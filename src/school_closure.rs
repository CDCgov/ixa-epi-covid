use std::{path::PathBuf, rc::Rc};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    geography::{Geography, GeographyType},
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, create_itinerary_transition_matrix,
    },
    pop_reader::{FIPSCode, StateCode},
    settings::{ContextSettingExt, Itinerary, Person, SettingCategory},
};
use ixa::{
    ExecutionPhase, IxaEvent, csv, impl_derived_property,
    prelude::*,
    triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolState(pub Option<StateCode>);
impl_derived_property!(SchoolState, Person, [Itinerary], [], |itinerary| {
    SchoolState(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| Some(code.0.state_code()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolCensusTract(pub Option<FIPSCode>);
impl_derived_property!(SchoolCensusTract, Person, [Itinerary], [], |itinerary| {
    SchoolCensusTract(
        itinerary.setting_ids[SettingCategory::School]
            .and_then(|code| code.0.community_code().ok()),
    )
});

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SchoolClosureRecord {
    pub geography: GeographyType,
    pub state: Option<StateCode>,
    pub census_tract: Option<FIPSCode>,
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SchoolClosuresFromFile {
    pub include: bool,
    pub filename: Option<PathBuf>,
}

#[derive(IxaEvent)]
struct SchoolClosure {
    active: bool,
    geography: Geography,
}

#[derive(Default)]
pub struct SchoolClosureData {
    state_school_closure: bool,
}

impl SchoolClosureData {
    pub fn new() -> Self {
        Self {
            state_school_closure: false,
        }
    }

    pub fn set_state_school_closure(&mut self, active: bool) {
        self.state_school_closure = active;
    }

    pub fn is_state_school_closure_active(&self) -> bool {
        self.state_school_closure
    }
}

define_data_plugin!(
    SchoolClosureDataPlugin,
    SchoolClosureData,
    SchoolClosureData::default()
);

pub trait SchoolClosureContextExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt + ContextSettingExt
{
    fn register_school_closure_itinerary_modifier(&mut self, geography: Geography) {
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier(geography);
        match geography {
            Geography::State(state) => {
                self.register_itinerary_modifier(SchoolState(Some(state)), itinerary_modifier);
            }
            Geography::CensusTract(fips_code) => {
                self.register_itinerary_modifier(
                    SchoolCensusTract(Some(fips_code)),
                    itinerary_modifier,
                );
            }
        }
    }
    fn remove_school_closure_itinerary_modifier(&mut self, geography: Geography) {
        match geography {
            Geography::State(state) => {
                self.remove_itinerary_modifier_by_property(SchoolState(Some(state)));
            }
            Geography::CensusTract(fips_code) => {
                self.remove_itinerary_modifier_by_property(SchoolCensusTract(Some(fips_code)));
            }
        }
    }
    fn setup_school_closure_triggers(
        &mut self,
        start_time: f64,
        end_time: f64,
        geography: Geography,
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

    fn setup_school_closure_itinerary_modification(&mut self) {
        self.subscribe_to_event(move |context, event: SchoolClosure| {
            if event.active {
                context.handle_school_closure_start(event.geography);
            } else {
                context.handle_school_closure_end(event.geography);
            }
        });
    }

    fn handle_school_closure_start(&mut self, geography: Geography) {
        self.register_school_closure_itinerary_modifier(geography);
        if let Geography::State(_) = geography {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            data.set_state_school_closure(true);
        }
    }

    fn handle_school_closure_end(&mut self, geography: Geography) {
        self.remove_school_closure_itinerary_modifier(geography);
        if let Geography::State(_) = geography {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            data.set_state_school_closure(false);
        }
    }

    fn is_state_school_closure_active(&self) -> bool {
        let data = self.get_data(SchoolClosureDataPlugin);
        data.is_state_school_closure_active()
    }
}
impl SchoolClosureContextExt for Context {}

fn create_school_closure_from_record(
    context: &mut Context,
    school_closure_record: SchoolClosureRecord,
) -> Result<(), ModelError> {
    let geography = match school_closure_record.geography {
        GeographyType::State => {
            if let Some(state) = school_closure_record.state {
                Geography::State(state)
            } else {
                return Err(ModelError::ModelError(
                    "State code is required for state-level school closure.".to_string(),
                ));
            }
        }
        GeographyType::CensusTract => {
            let census_tract = school_closure_record.census_tract.ok_or_else(|| {
                ModelError::ModelError(
                    "Census tract code is required for census tract-level school closure."
                        .to_string(),
                )
            })?;
            Geography::CensusTract(census_tract)
        }
    };
    println!(
        "Setting up school closure triggers for {:?} from {} to {}",
        geography, school_closure_record.start_time, school_closure_record.end_time
    );
    context.setup_school_closure_triggers(
        school_closure_record.start_time,
        school_closure_record.end_time,
        geography,
    );
    Ok(())
}

fn read_school_closures_file(
    context: &mut Context,
    school_closure_file: PathBuf,
) -> Result<(), ModelError> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(school_closure_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: SchoolClosureRecord = raw_record.deserialize(Some(&headers))?;
        println!("Read school closure record: {:?}", record);
        create_school_closure_from_record(context, record)?;
    }
    Ok(())
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
    load_school_closures(context)?;
    context.setup_school_closure_itinerary_modification();
    Ok(())
}

fn define_virtual_school_closure_itinerary_modifier(
    geography: Geography,
) -> ItineraryTransitionMatrix {
    let matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ];
    let acceptance_function: Option<AcceptanceFunction> =
        Some(Rc::new(move |context, _person| match geography {
            Geography::State(_) => context.is_state_school_closure_active(),
            Geography::CensusTract(_) => !context.is_state_school_closure_active(),
        }));
    create_itinerary_transition_matrix(None, Some(matrix), acceptance_function)
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;

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
        let g1 = Geography::State(1);
        context.setup_school_closure_triggers(1.0, 2.0, g1);
        context.setup_school_closure_itinerary_modification();
        context.subscribe_to_event(move |cxt, event: SchoolClosure| {
            if event.active && event.geography == g1 {
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
        let school_code = make_school_id(b"16037960200002");
        let g1 = Geography::State(school_code.0.state_code());
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_school_closure_itinerary_modification();
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
                geography: g1,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: g1,
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

    #[test]
    fn test_overlapping_school_closures() {
        let mut context = setup();
        context.setup_school_closure_itinerary_modification();
        let school_code = make_school_id(b"16037960200002");
        let g1 = Geography::State(school_code.0.state_code());
        let g2 = Geography::CensusTract(school_code.0.community_code().unwrap());
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
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
                geography: g2,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                geography: g1,
            });
        });

        context.add_plan(3.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: g1,
            });
        });
        context.add_plan(4.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: g2,
            });
        });

        context.add_plan(0.0, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert!(!data.is_state_school_closure_active());
        });
        context.add_plan(1.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert!(!data.is_state_school_closure_active());
        });
        context.add_plan(2.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert!(data.is_state_school_closure_active());
        });
        context.add_plan(3.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert!(!data.is_state_school_closure_active());
        });
        context.add_plan(4.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert!(!data.is_state_school_closure_active());
        });
        context.execute();
    }
}
