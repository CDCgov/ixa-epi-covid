use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    geography::Geography,
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, create_itinerary_transition_matrix,
    },
    pop_reader::{FIPSCode, StateCode},
    settings::{ContextSettingExt, Itinerary, Person, SETTING_COUNT, SettingCategory},
};
use ixa::{
    ExecutionPhase, HashMap, HashMapExt, IxaEvent, impl_derived_property,
    prelude::*,
    triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};

define_rng!(InterventionRng);

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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkState(pub Option<StateCode>);
impl_derived_property!(WorkState, Person, [Itinerary], [], |itinerary| {
    WorkState(
        itinerary.setting_ids[SettingCategory::Work]
            .map(|code| Some(code.0.state_code()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkCounty(pub Option<FIPSCode>);
impl_derived_property!(WorkCounty, Person, [Itinerary], [], |itinerary| {
    WorkCounty(
        itinerary.setting_ids[SettingCategory::Work]
            .and_then(|code| code.0.state_county_code().ok()),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct HomeState(pub Option<StateCode>);
impl_derived_property!(HomeState, Person, [Itinerary], [], |itinerary| {
    HomeState(
        itinerary.setting_ids[SettingCategory::Home]
            .map(|code| Some(code.0.state_code()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct HomeCounty(pub Option<FIPSCode>);
impl_derived_property!(HomeCounty, Person, [Itinerary], [], |itinerary| {
    HomeCounty(
        itinerary.setting_ids[SettingCategory::Home]
            .and_then(|code| code.0.state_county_code().ok()),
    )
});

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize, Eq, Hash)]
pub enum Modifier {
    SchoolClosure,
    CommunityMobilityReduction,
    WorkplaceMobilityReduction,
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct ModifierSpecification {
    home: Option<[f64; SETTING_COUNT]>,
    school: Option<[f64; SETTING_COUNT]>,
    work: Option<[f64; SETTING_COUNT]>,
    community: Option<[f64; SETTING_COUNT]>,
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Intervention {
    modifier: Modifier,
    geography: Geography,
    acceptance_probability: f64,
    activation_time: f64,
    duration: Option<f64>,
    override_modifiers: Option<ModifierSpecification>,
}

#[derive(IxaEvent)]
struct InterventionEvent {
    active: bool,
    intervention: Intervention,
}

#[derive(Default)]
pub struct InterventionData {
    active_interventions: HashMap<(Modifier, Geography), bool>,
}

impl InterventionData {
    pub fn new() -> Self {
        Self {
            active_interventions: HashMap::<(Modifier, Geography), bool>::new(),
        }
    }

    pub fn activate_intervention(&mut self, intervention: Intervention) {
        self.active_interventions
            .insert((intervention.modifier, intervention.geography), true);
    }

    pub fn deactivate_intervention(&mut self, intervention: Intervention) {
        self.active_interventions
            .insert((intervention.modifier, intervention.geography), false);
    }

    pub fn is_intervention_active(&self, modifier: Modifier, geography: Geography) -> bool {
        *self
            .active_interventions
            .get(&(modifier, geography))
            .unwrap_or(&false)
    }
}

define_data_plugin!(
    InterventionDataPlugin,
    InterventionData,
    InterventionData::default()
);

pub trait SchoolClosureContextExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt + ContextSettingExt
{
    fn setup_intervention_triggers(&mut self, intervention: Intervention) {
        let start_trigger =
            TimeTrigger::at_phase(intervention.activation_time, ExecutionPhase::Last).emit_value(
                InterventionEvent {
                    active: true,
                    intervention,
                },
            );
        self.register_trigger(start_trigger);

        if let Some(duration) = intervention.duration {
            let end_trigger = TimeTrigger::at_phase(
                intervention.activation_time + duration,
                ExecutionPhase::Last,
            )
            .emit_value(InterventionEvent {
                active: false,
                intervention,
            });
            self.register_trigger(end_trigger);
        }
    }

    fn setup_intervention_trigger_event_subscription(&mut self) {
        self.subscribe_to_event(move |context, event: InterventionEvent| {
            if event.active {
                context
                    .register_intervention_itinerary_modifier(event.intervention)
                    .unwrap();
                let data = context.get_data_mut(InterventionDataPlugin);
                data.activate_intervention(event.intervention);
            } else {
                context
                    .remove_intervention_itinerary_modifier(event.intervention)
                    .unwrap();
                let data = context.get_data_mut(InterventionDataPlugin);
                data.deactivate_intervention(event.intervention);
            }
        });
    }

    fn register_intervention_itinerary_modifier(
        &mut self,
        intervention: Intervention,
    ) -> Result<(), ModelError> {
        let itinerary_modifier = self.define_intervention_itinerary_modifier(intervention);
        match (intervention.modifier, intervention.geography) {
            (Modifier::SchoolClosure, Geography::State(code)) => {
                self.register_itinerary_modifier(SchoolState(Some(code)), itinerary_modifier);
            }
            (Modifier::SchoolClosure, Geography::County(code)) => {
                self.register_itinerary_modifier(SchoolCounty(Some(code)), itinerary_modifier);
            }
            (Modifier::WorkplaceMobilityReduction, Geography::State(code)) => {
                self.register_itinerary_modifier(WorkState(Some(code)), itinerary_modifier);
            }
            (Modifier::WorkplaceMobilityReduction, Geography::County(code)) => {
                self.register_itinerary_modifier(WorkCounty(Some(code)), itinerary_modifier);
            }
            (Modifier::CommunityMobilityReduction, Geography::State(code)) => {
                self.register_itinerary_modifier(HomeState(Some(code)), itinerary_modifier);
            }
            (Modifier::CommunityMobilityReduction, Geography::County(code)) => {
                self.register_itinerary_modifier(HomeCounty(Some(code)), itinerary_modifier);
            }
        }
        Ok(())
    }
    fn remove_intervention_itinerary_modifier(
        &mut self,
        intervention: Intervention,
    ) -> Result<(), ModelError> {
        match (intervention.modifier, intervention.geography) {
            (Modifier::SchoolClosure, Geography::State(code)) => {
                self.remove_itinerary_modifier_by_property(SchoolState(Some(code)));
            }
            (Modifier::SchoolClosure, Geography::County(code)) => {
                self.remove_itinerary_modifier_by_property(SchoolCounty(Some(code)));
            }
            (Modifier::WorkplaceMobilityReduction, Geography::State(code)) => {
                self.remove_itinerary_modifier_by_property(WorkState(Some(code)));
            }
            (Modifier::WorkplaceMobilityReduction, Geography::County(code)) => {
                self.remove_itinerary_modifier_by_property(WorkCounty(Some(code)));
            }
            (Modifier::CommunityMobilityReduction, Geography::State(code)) => {
                self.remove_itinerary_modifier_by_property(HomeState(Some(code)));
            }
            (Modifier::CommunityMobilityReduction, Geography::County(code)) => {
                self.remove_itinerary_modifier_by_property(HomeCounty(Some(code)));
            }
        }
        Ok(())
    }

    fn is_intervention_active(&self, modifier: Modifier, geography: Geography) -> bool {
        let data = self.get_data(InterventionDataPlugin);
        data.is_intervention_active(modifier, geography)
    }

    fn define_intervention_itinerary_modifier(
        &self,
        intervention: Intervention,
    ) -> ItineraryTransitionMatrix {
        let default_modifers = self.get_params().default_modifiers.clone();
        let modifier_params = default_modifers
            .get(&intervention.modifier)
            .expect("Modifier parameters not found");

        let mut matrix = [[0.0; SETTING_COUNT]; SETTING_COUNT];
        let overrides = intervention.override_modifiers.as_ref();
        let settings = [
            (SettingCategory::Home, modifier_params.home, overrides.and_then(|m| m.home)),
            (SettingCategory::School, modifier_params.school, overrides.and_then(|m| m.school)),
            (SettingCategory::Work, modifier_params.work, overrides.and_then(|m| m.work)),
            (SettingCategory::Community, modifier_params.community, overrides.and_then(|m| m.community)),
        ];

        for (category, default_val, override_val) in settings {
            if let Some(val) = override_val.or(default_val) {
                matrix[category] = val;
            }
        }

        let acceptance_function: Option<AcceptanceFunction> =
            Some(Box::new(move |context, _person| {
                match intervention.geography {
                    Geography::State(_) => {
                        context
                            .is_intervention_active(intervention.modifier, intervention.geography)
                            && context
                                .sample_bool(InterventionRng, intervention.acceptance_probability)
                    }
                    Geography::County(code) => {
                        !context.is_intervention_active(
                            Modifier::SchoolClosure,
                            Geography::State(code.state_code()),
                        ) && context
                            .is_intervention_active(Modifier::SchoolClosure, intervention.geography)
                            && context
                                .sample_bool(InterventionRng, intervention.acceptance_probability)
                    }
                }
            }));
        create_itinerary_transition_matrix(None, Some(matrix), acceptance_function)
    }
}
impl SchoolClosureContextExt for Context {}

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    let Params { interventions, .. } = context.get_params().clone();
    for intervention in interventions {
        context.setup_intervention_triggers(intervention);
    }
    context.setup_intervention_trigger_event_subscription();
    Ok(())
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
            default_modifiers: HashMap::from_iter([
                (
                    Modifier::SchoolClosure,
                    ModifierSpecification {
                        home: None,
                        school: Some([1.0, 0.0, 0.0, 0.0]),
                        work: None,
                        community: None,
                    },
                ),
                (
                    Modifier::WorkplaceMobilityReduction,
                    ModifierSpecification {
                        home: None,
                        school: None,
                        work: Some([1.0, 0.0, 0.0, 0.0]),
                        community: None,
                    },
                ),
                (
                    Modifier::CommunityMobilityReduction,
                    ModifierSpecification {
                        home: None,
                        school: None,
                        work: None,
                        community: Some([1.0, 0.0, 0.0, 0.0]),
                    },
                ),
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
    fn test_setup_intervention_triggers() {
        let mut context = setup();
        let school_closures_starts = Rc::new(RefCell::new(0));
        let school_closures_starts_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_starts);
        let school_closures_ends = Rc::new(RefCell::new(0));
        let school_closures_ends_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_ends);
        let g1 = Geography::State(1);
        let intervention = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 1.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        context.setup_intervention_triggers(intervention);
        context.setup_intervention_trigger_event_subscription();
        context.subscribe_to_event(move |cxt, event: InterventionEvent| {
            if event.active && event.intervention.geography == g1 {
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
        let intervention = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 1.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        context.add_plan(1.0, move |context| {
            context.emit_event(InterventionEvent {
                active: true,
                intervention,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(InterventionEvent {
                active: false,
                intervention,
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
