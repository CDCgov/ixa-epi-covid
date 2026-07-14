use std::path::PathBuf;

use ixa::{
    ExecutionPhase, HashMap, IxaEvent, csv, impl_derived_property, prelude::*, triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params, error::ModelError, itinerary_manager::ContextItineraryModifierExt, itinerary_modifiers::{ItineraryTransitionMatrix, define_itinerary_modifier}, pop_reader::FIPSCode, population_loader::SchoolId, settings::{ContextSettingExt, Itinerary, Person, SettingCategory, SettingCode},
};

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SchoolClosureRecord {
    pub geography: FIPSCode,
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SchoolClosuresFromFile {
    pub include: bool,
    pub filename: Option<PathBuf>,
}


#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct GoesToSchool(pub bool);

impl_derived_property!(GoesToSchool, Person, [Itinerary], [], |itinerary| {
    GoesToSchool(itinerary.setting_ids[SettingCategory::School].is_some())
});

define_multi_property!(Person, (SchoolId, GoesToSchool));

#[derive(IxaEvent)]
struct SchoolClosure {
    active: bool,
    geography: FIPSCode,
}

#[derive(Default)]
pub struct GeographicMembership {
    members: HashMap<FIPSCode, Vec<SettingCode>>,
}

impl GeographicMembership {
    pub fn new() -> Self {
        Self {
            members: HashMap::default(),
        }
    }

    pub fn add_member(&mut self, geography: FIPSCode, setting_code: SettingCode) {
        let members = self.members.entry(geography).or_default();
        if !members.contains(&setting_code) {
            members.push(setting_code);
        }
    }

    pub fn get_members(&self, geography: FIPSCode) -> Option<&Vec<SettingCode>> {
        self.members.get(&geography)
    }
}

define_data_plugin!(
    GeographicMembershipPlugin, 
    GeographicMembership, 
    GeographicMembership::default()
);

pub trait SchoolClosureContextExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt + ContextSettingExt
{   
    fn add_all_schools_to_geography(&mut self, geography: FIPSCode) {
        
        let all_schools: Vec<_> = self.get_all_settings()
            .into_iter()
            .filter(|code| code.category() == SettingCategory::School)
            .collect();
        let container = self.get_data_mut(GeographicMembershipPlugin);
        for school in all_schools {
            container.add_member(geography, school);
        }
    }

    fn get_schools_in_geography(&self, geography: FIPSCode) -> Vec<SettingCode> {
        let container = self.get_data(GeographicMembershipPlugin);
        container.get_members(geography).cloned().unwrap_or_default()
    }

    fn setup_school_closure_triggers(&mut self, start_time: f64, end_time: f64, geography: FIPSCode) {
        let start_trigger = TimeTrigger::at_phase(start_time, ExecutionPhase::Last)
            .emit_value(SchoolClosure { active: true, geography });
        let end_trigger = TimeTrigger::at_phase(end_time, ExecutionPhase::Last)
        .emit_value(SchoolClosure { active: false, geography });

        self.register_trigger(start_trigger);
        self.register_trigger(end_trigger);
    }

    fn setup_school_closure_itinerary_modification(
        &mut self,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        self.subscribe_to_event(move |context, event: SchoolClosure| {
            let schools:Vec<SettingCode> = context.get_schools_in_geography(event.geography);
            if event.active {
                for school in schools {
                    context.register_itinerary_modifier((SchoolId(Some(school)),GoesToSchool(true)), itinerary_modifier);
                }
            } else {
                for school in schools {
                    context.register_itinerary_modifier((SchoolId(Some(school)),GoesToSchool(true)), itinerary_modifier);
                }
            }
        });
    }
}
impl SchoolClosureContextExt for Context {}

fn create_school_closure_from_record(
    context: &mut Context,
    school_closure_record: SchoolClosureRecord,
) -> Result<(), ModelError> {
    context.add_all_schools_to_geography(school_closure_record.geography);
    context.setup_school_closure_triggers(school_closure_record.start_time, school_closure_record.end_time, school_closure_record.geography);
    Ok(())
}

fn read_school_closures_file(
    context: &mut Context,
    school_closure_file: PathBuf,
) -> Result<(), ModelError> {
    let mut reader = csv::Reader::from_path(school_closure_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: SchoolClosureRecord = raw_record.deserialize(Some(&headers))?;
        create_school_closure_from_record(context, record)?;
    }
    Ok(())
}


fn load_school_closures(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        school_closures,
        ..
    } = context.get_params();
    if school_closures.include {
        if let Some(filename) = &school_closures.filename {
            read_school_closures_file(context, filename.clone())?;
        } else {
            return Err(ModelError::ModelError(
                "School closures are turned on but no filename was provided.".to_string(),
            ));
        }
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), ModelError>{
    let school_closure_itinerary_modifier = define_virutal_school_closure_itinerary_modifier();
    load_school_closures(context)?;
    context.setup_school_closure_itinerary_modification(school_closure_itinerary_modifier);
    Ok(())
}

fn define_virutal_school_closure_itinerary_modifier() -> ItineraryTransitionMatrix {
    let weekend_matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ];

    define_itinerary_modifier(None, Some(weekend_matrix))
}

// #[cfg(test)]
// mod test {
//     use std::cell::RefCell;
//     use std::rc::Rc;

//     use super::*;
//     use crate::Age;
//     use crate::parameters::{GlobalParams, Params, SettingProperties};
//     use crate::pop_reader::parser::parse_fips_school_id;
//     use crate::setting_code::SettingCode;
//     use crate::settings::SettingCategory;
//     use ixa::HashMap;

//     fn make_school_id(school_id: &[u8]) -> SettingCode {
//         SettingCode(parse_fips_school_id(school_id).unwrap().1)
//     }

//     fn setup() -> Context {
//         let mut context = Context::new();
//         let parameters = Params {
//             // We need to specify an itinerary split here even though we don't draw people from
//             // itineraries because `load_synth_population` calls `create_itinerary` for each person,
//             // and that function requires an itinerary write function to be set.
//             settings_properties: HashMap::from_iter(
//                 [
//                     (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
//                     (SettingCategory::School, SettingProperties { alpha: 0.0 }),
//                     (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
//                     (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
//                 ]
//                 .into_iter()
//                 .collect::<HashMap<_, _>>(),
//             ),
//             itinerary_ratios: HashMap::from_iter([
//                 (SettingCategory::Home, 0.25),
//                 (SettingCategory::School, 0.25),
//                 (SettingCategory::Work, 0.25),
//                 (SettingCategory::Community, 0.25),
//             ]),
//             ..Default::default()
//         };
//         context
//             .set_global_property_value(GlobalParams, parameters)
//             .unwrap();
//         crate::settings::init(&mut context).unwrap();
//         context
//     }

//     #[test]
//     fn test_weekend_triggers() {
//         let mut context = setup();
//         let weekend_starts = Rc::new(RefCell::new(0));
//         let weekend_starts_clone: Rc<RefCell<usize>> = Rc::clone(&weekend_starts);
//         let weekend_ends = Rc::new(RefCell::new(0));
//         let weekend_ends_clone: Rc<RefCell<usize>> = Rc::clone(&weekend_ends);
//         context.setup_weekend_triggers(3.0);
//         context.subscribe_to_event(move |cxt, event: Weekend| {
//             if event.active {
//                 assert_eq!(cxt.get_current_time() % 7.0, 3.0);
//                 *weekend_starts_clone.borrow_mut() += 1;
//             } else {
//                 assert_eq!(cxt.get_current_time() % 7.0, 5.0);
//                 *weekend_ends_clone.borrow_mut() += 1;
//             }
//         });
//         context.add_plan_with_phase(18.0, ixa::Context::shutdown, ExecutionPhase::Last);
//         context.execute();
//         #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
//         let observed_weekend_starts = *weekend_starts.borrow();
//         let observed_weekend_ends = *weekend_ends.borrow();
//         // weekend starts on day 3, 10, and 17
//         // weekend ends on day 5, 12, and 19
//         assert_eq!(observed_weekend_starts, 3);
//         assert_eq!(observed_weekend_ends, 2);
//     }

//     #[test]
//     fn test_itinerary_modification_registration() {
//         let mut context = setup();
//         let weekend_modifier = define_weekend_itinerary_modifier(0.5, 0.5);
//         let school_code = make_school_id(b"16037960200002");
//         let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
//         // context.register_itinerary_modifier(GoesToSchool(true), weekend_modifier);
//         context.setup_weekend_itinerary_modification(weekend_modifier);
//         context.set_property(
//             p1,
//             Itinerary {
//                 setting_ids: [None, None, Some(school_code), None],
//                 itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
//             },
//         );
//         context.add_plan(1.0, move |context| {
//             context.emit_event(Weekend { active: true });
//         });
//         context.add_plan(2.0, move |context| {
//             context.emit_event(Weekend { active: false });
//         });

//         context.add_plan(0.0, move |context| {
//             let itinerary = context.get_itinerary(p1);
//             assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
//         });
//         context.add_plan(1.5, move |context| {
//             let itinerary = context.get_itinerary(p1);
//             assert_eq!(itinerary, [0.55, 0.0, 0.0, 0.45]);
//         });
//         context.add_plan(3.0, move |context| {
//             let itinerary = context.get_itinerary(p1);
//             assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
//         });
//         context.execute();
//     }
// }