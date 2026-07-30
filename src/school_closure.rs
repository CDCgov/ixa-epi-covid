use std::path::PathBuf;

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    itinerary_manager::ContextItineraryModifierExt,
    itinerary_modifiers::{ItineraryTransitionMatrix, define_itinerary_modifier},
    pop_reader::{FIPSCode, parser::parse_fips_community_id},
    population_loader::SchoolId,
    settings::{ContextSettingExt, Itinerary, Person, SettingCategory},
};
use ixa::{
    ExecutionPhase, IxaEvent, csv, impl_derived_property,
    prelude::*,
    triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolCensusTract(pub Option<FIPSCode>);
impl_derived_property!(SchoolCensusTract, Person, [Itinerary], [], |itinerary| {
    SchoolCensusTract(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| Some(code.extract_community().0))
            .unwrap_or(None),
    )
});

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SchoolClosureRecord {
    pub census_tract: FIPSCode,
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SchoolClosuresFromFile {
    pub include: bool,
    pub filename: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Student(pub bool);

impl_derived_property!(Student, Person, [Itinerary], [], |itinerary| {
    Student(itinerary.setting_ids[SettingCategory::School].is_some())
});

define_multi_property!(Person, (SchoolId, Student));

#[derive(IxaEvent)]
struct SchoolClosure {
    active: bool,
    geography: FIPSCode,
}

pub trait SchoolClosureContextExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt + ContextSettingExt
{
    fn setup_school_closure_triggers(
        &mut self,
        start_time: f64,
        end_time: f64,
        geography: FIPSCode,
    ) {
        let start_trigger =
            TimeTrigger::at_phase(start_time, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: true,
                geography,
            });
        let end_trigger =
            TimeTrigger::at_phase(end_time, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: false,
                geography,
            });

        self.register_trigger(start_trigger);
        self.register_trigger(end_trigger);
    }

    fn setup_school_closure_itinerary_modification(
        &mut self,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        self.subscribe_to_event(move |context, event: SchoolClosure| {
            if event.active {
                println!(
                    "School closure event triggered for census tract {} at time {}",
                    event.geography,
                    context.get_current_time()
                );
                context.register_itinerary_modifier(
                    SchoolCensusTract(Some(event.geography)),
                    itinerary_modifier,
                );
            } else {
                println!(
                    "School closure event ended for census tract {} at time {}",
                    event.geography,
                    context.get_current_time()
                );
                context.remove_itinerary_modifier_by_property(SchoolCensusTract(Some(
                    event.geography,
                )));
            }
        });
    }
}
impl SchoolClosureContextExt for Context {}

fn create_school_closure_from_record(
    context: &mut Context,
    school_closure_record: SchoolClosureRecord,
) -> Result<(), ModelError> {
    println!(
        "Creating school closure from record: {:?}",
        school_closure_record
    );
    context.setup_school_closure_triggers(
        school_closure_record.start_time,
        school_closure_record.end_time,
        school_closure_record.census_tract,
    );
    Ok(())
}

fn read_school_closures_file(
    context: &mut Context,
    school_closure_file: PathBuf,
) -> Result<(), ModelError> {
    println!(
        "Reading school closures from file: {}",
        school_closure_file.display()
    );
    let mut reader = csv::Reader::from_path(school_closure_file)?;
    let mut raw_record = csv::ByteRecord::new();
    while reader.read_byte_record(&mut raw_record)? {
        let census_tract = raw_record.get(0).ok_or_else(|| {
            ModelError::ModelError("Missing census tract in school closure record".to_string())
        })?;
        let census_tract_code = parse_fips_community_id(census_tract).unwrap().1;
        println!(
            "Parsed census tract code: {} from raw value: {:?}",
            census_tract_code, census_tract
        );
        let record: SchoolClosureRecord = parse_school_closure_record(&raw_record)?;
        create_school_closure_from_record(context, record)?;
    }
    Ok(())
}

fn parse_school_closure_record(
    record: &csv::ByteRecord,
) -> Result<SchoolClosureRecord, ModelError> {
    let census_tract = record.get(0).ok_or_else(|| {
        ModelError::ModelError("Missing census tract in school closure record".to_string())
    })?;

    let parse_time = |index, field: &str| {
        let value = record.get(index).ok_or_else(|| {
            ModelError::ModelError(format!("Missing {field} in school closure record"))
        })?;

        std::str::from_utf8(value)
            .map_err(|e| {
                ModelError::ModelError(format!(
                    "Invalid {field} encoding in school closure record: {e}"
                ))
            })?
            .parse::<f64>()
            .map_err(|e| {
                ModelError::ModelError(format!("Invalid {field} in school closure record: {e}"))
            })
    };

    Ok(SchoolClosureRecord {
        census_tract: parse_fips_community_id(census_tract).unwrap().1,
        start_time: parse_time(1, "start time")?,
        end_time: parse_time(2, "end time")?,
    })
}

fn load_school_closures(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        school_closures, ..
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

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    let school_closure_itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
    load_school_closures(context)?;
    context.setup_school_closure_itinerary_modification(school_closure_itinerary_modifier);
    Ok(())
}

fn define_virtual_school_closure_itinerary_modifier() -> ItineraryTransitionMatrix {
    let weekend_matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ];

    define_itinerary_modifier(None, Some(weekend_matrix))
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use ixa::HashMap;

    use crate::{
        Age,
        itinerary_manager::ContextItineraryModifierExt,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::parser::parse_fips_school_id,
        settings::SettingCode,
    };

    use super::*;
    #[allow(dead_code)]
    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Work, 0.25),
                (SettingCategory::Community, 0.25),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_setup_school_closure_triggers() {
        let mut context = setup();
        let school_closures_starts = Rc::new(RefCell::new(0));
        let school_closures_starts_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_starts);
        let school_closures_ends = Rc::new(RefCell::new(0));
        let school_closures_ends_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_ends);
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
        let tract_fips_code = make_school_id(b"16037960200002").extract_community().0;
        context.setup_school_closure_triggers(1.0, 2.0, tract_fips_code);
        context.setup_school_closure_itinerary_modification(itinerary_modifier);
        context.subscribe_to_event(move |cxt, event: SchoolClosure| {
            if event.active && event.geography == tract_fips_code {
                assert_eq!(cxt.get_current_time(), 1.0);
                *school_closures_starts_clone.borrow_mut() += 1;
            } else {
                assert_eq!(cxt.get_current_time(), 2.0);
                *school_closures_ends_clone.borrow_mut() += 1;
            }
        });
        context.add_plan_with_phase(3.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_school_closures_starts = *school_closures_starts.borrow();
        let observed_school_closures_ends = *school_closures_ends.borrow();
        // school closures start on day 1
        // school closures end on day 2
        assert_eq!(observed_school_closures_starts, 1);
        assert_eq!(observed_school_closures_ends, 1);
    }

    #[test]
    fn test_setup_school_closure_itinerary_modification() {
        let mut context = setup();
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
        let school_code = make_school_id(b"16037960200002");
        let tract_fips_code = school_code.extract_community().0;
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_school_closure_itinerary_modification(itinerary_modifier);
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        context.add_plan(1.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                geography: tract_fips_code,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: tract_fips_code,
            });
        });

        context.add_plan(0.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.add_plan(1.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.8, 0.0, 0.0, 0.2]);
        });
        context.add_plan(3.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.execute();
    }
}
