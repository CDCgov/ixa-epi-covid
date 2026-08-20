use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, create_itinerary_transition_matrix,
    },
    pop_reader::{FIPSCode, StateCode},
    schools::school_geography::Geography,
    settings::{ContextSettingExt, Itinerary, Person, SettingCategory},
};
use ixa::{
    ExecutionPhase, IxaEvent, impl_derived_property,
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
pub struct SchoolCounty(pub Option<FIPSCode>);
impl_derived_property!(SchoolCounty, Person, [Itinerary], [], |itinerary| {
    SchoolCounty(
        itinerary.setting_ids[SettingCategory::School]
            .and_then(|code| code.0.state_county_code().ok()),
    )
});

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SchoolClosureParameters {
    pub geography: Geography,
    pub activates_at: f64,
    pub deactivates_at: f64,
}

#[derive(IxaEvent)]
struct SchoolClosure {
    active: bool,
    geography: Geography,
}

#[derive(Default)]
pub struct SchoolClosureData {
    active_state_school_closure: bool,
}

impl SchoolClosureData {
    pub fn new() -> Self {
        Self {
            active_state_school_closure: false,
        }
    }

    pub fn set_state_school_closure(&mut self, active: bool) {
        self.active_state_school_closure = active;
    }

    pub fn is_state_school_closure_active(&self) -> bool {
        self.active_state_school_closure
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
    fn register_school_closure_itinerary_modifier(
        &mut self,
        geography: Geography,
    ) -> Result<(), ModelError> {
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier(geography);
        match geography {
            Geography::State(code) => {
                self.register_itinerary_modifier(SchoolState(Some(code)), itinerary_modifier);
            }
            Geography::County(code) => {
                self.register_itinerary_modifier(SchoolCounty(Some(code)), itinerary_modifier);
            }
        }
        Ok(())
    }
    fn remove_school_closure_itinerary_modifier(
        &mut self,
        geography: Geography,
    ) -> Result<(), ModelError> {
        match geography {
            Geography::State(code) => {
                self.remove_itinerary_modifier_by_property(SchoolState(Some(code)));
            }
            Geography::County(code) => {
                self.remove_itinerary_modifier_by_property(SchoolCounty(Some(code)));
            }
        }
        Ok(())
    }
    fn setup_school_closure_triggers(
        &mut self,
        activates_at: f64,
        deactivates_at: f64,
        geography: Geography,
    ) {
        let start_trigger =
            TimeTrigger::at_phase(activates_at, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: true,
                geography,
            });
        let end_trigger =
            TimeTrigger::at_phase(deactivates_at, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: false,
                geography,
            });

        self.register_trigger(start_trigger);
        self.register_trigger(end_trigger);
    }

    fn setup_school_closure_trigger_event_subscription(&mut self) {
        self.subscribe_to_event(move |context, event: SchoolClosure| {
            if event.active {
                context.handle_school_closure_start(event.geography);
            } else {
                context.handle_school_closure_end(event.geography);
            }
        });
    }

    fn handle_school_closure_start(&mut self, geography: Geography) {
        self.register_school_closure_itinerary_modifier(geography)
            .unwrap();
        if let Geography::State(_) = geography {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            data.set_state_school_closure(true);
        }
    }

    fn handle_school_closure_end(&mut self, geography: Geography) {
        self.remove_school_closure_itinerary_modifier(geography)
            .unwrap();
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

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        school_closures, ..
    } = context.get_params().clone();
    for record in school_closures {
        context.setup_school_closure_triggers(
            record.activates_at,
            record.deactivates_at,
            record.geography,
        );
    }
    context.setup_school_closure_trigger_event_subscription();
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
        Some(Box::new(move |context, _person| match geography {
            Geography::State(_) => context.is_state_school_closure_active(),
            Geography::County(_) => !context.is_state_school_closure_active(),
        }));
    create_itinerary_transition_matrix(None, Some(matrix), acceptance_function)
}

#[cfg(test)]
mod test {
    use crate::{
        Age,
        itinerary_manager::ContextItineraryModifierExt,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::parser::parse_fips_school_id,
        settings::SettingCode,
    };
    use ixa::HashMap;
    use std::{cell::RefCell, rc::Rc};

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
        context.setup_school_closure_trigger_event_subscription();
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
        context.setup_school_closure_trigger_event_subscription();
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
}
