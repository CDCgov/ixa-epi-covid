use ixa::{prelude::*, prelude_for_plugins::IxaEvent};
use serde::Serialize;

use crate::{
    ContextParametersExt,
    custom_events::{ContextCustomEventExt, HospitalizationThresholdEvent},
    settings::{ContextSettingExt, Person},
};

#[derive(IxaEvent, Copy, Clone, Debug)]
pub struct PolicyEvent {
    active: bool,
    policy_trigger: PolicyTrigger,
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub enum PolicyTrigger {
    // this is where the trigger attributes will live like start and end
    TimeTrigger {
        start_time: f64,
        end_time: Option<f64>,
    },
    // this is an example and uses a custom event that is generated tracking hospitalizations
    HospitalizationThresholdTrigger {
        threshold: usize,
    },
    // this is for policies that are active for the entire simulation and don't need a trigger event to activate them.
    OnSimulationInitializationTrigger,
}

impl PolicyTrigger {
    // This is where future trait methods will come from.
    // Any time there is a match can it live inside the enum
    fn emit_policy_event(&self, context: &mut Context) {
        match self {
            PolicyTrigger::TimeTrigger {
                start_time,
                end_time,
            } => {
                let policy_trigger = *self;
                context.add_plan(*start_time, move |context| {
                    context.emit_event(PolicyEvent {
                        active: true,
                        policy_trigger,
                    });
                });
                if let Some(end_time) = end_time {
                    context.add_plan(*end_time, move |context| {
                        context.emit_event(PolicyEvent {
                            active: false,
                            policy_trigger,
                        });
                    });
                }
            }
            PolicyTrigger::HospitalizationThresholdTrigger { threshold } => {
                let threshold = *threshold;
                let policy_trigger = *self;
                context.set_hospitalization_threshold(threshold);
                context.subscribe_to_event::<HospitalizationThresholdEvent>(
                    move |context, event| {
                        if event.above_threshold {
                            context.emit_event(PolicyEvent {
                                active: true,
                                policy_trigger,
                            });
                        } else {
                            context.emit_event(PolicyEvent {
                                active: false,
                                policy_trigger,
                            });
                        }
                    },
                );
            }
            PolicyTrigger::OnSimulationInitializationTrigger => {
                let policy_trigger = *self;
                context.emit_event(PolicyEvent {
                    active: true,
                    policy_trigger,
                });
            }
        }
    }
}

pub trait InterventionTrait: std::fmt::Debug + Copy + 'static {
    fn activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
    fn deactivate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct Policy<P, I>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    I: InterventionTrait,
{
    trigger: PolicyTrigger,
    intervention: I,
    group: P,
}

pub trait ContextPolicyExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt
{
    fn intitialize_policy_trigger(&mut self, trigger: &PolicyTrigger);
    fn add_policy<P, I>(&mut self, policy: Policy<P, I>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        I: InterventionTrait,
    {
        let policy_trigger = policy.trigger;
        let group_property = policy.group;
        let intervention = policy.intervention;

        self.subscribe_to_event::<PolicyEvent>(move |context, event| {
            let policy_trigger_event = event.policy_trigger;
            if event.active && policy_trigger_event == policy_trigger {
                intervention.activate(context, group_property);
            } else {
                intervention.deactivate(context, group_property);
            }
        });

        // logic to generate events which start the policy.
        // Subscriptions happens first because some policies might emit an event right away
        self.intitialize_policy_trigger(&policy_trigger);
    }
}
impl ContextPolicyExt for Context {
    fn intitialize_policy_trigger(&mut self, trigger: &PolicyTrigger) {
        trigger.emit_policy_event(self);
    }
}
#[cfg(test)]
mod tests {
    use ixa::{Context, HashMap};

    use crate::{
        Age, Params,
        itinerary_manager::{ItineraryModifier, define_itinerary_modifier},
        parameters::{GlobalParams, SettingProperties},
        pop_reader::{
            FIPSCode, PopulationReaderSettingCategory,
            parser::{parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id},
        },
        population_loader::Alive,
        settings::{PersonId, SettingCategory, SettingCode},
        symptom_status_manager::{SymptomData, SymptomStatus},
    };

    use super::*;
    fn make_home_id(home_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_home_id(home_id).unwrap().1)
    }

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn make_workplace_id(workplace_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_workplace_id(workplace_id).unwrap().1)
    }

    fn make_community_id(home_id: &[u8]) -> SettingCode {
        let home_id = make_home_id(home_id).0;
        SettingCode(
            FIPSCode::with_category(
                home_id.state_code(),
                home_id.county_code(),
                home_id.census_tract_code(),
                PopulationReaderSettingCategory::CensusTract.encode(),
            )
            .unwrap(),
        )
    }

    fn add_person_to_test_settings(context: &mut Context, person: PersonId) {
        let home_code = make_home_id(b"160379602000010");
        let school_code = make_school_id(b"160379602000010");
        let workplace_code = make_workplace_id(b"160379602000010");
        let community_code = make_community_id(b"160379602000010");
        context.add_person_to_settings(
            person,
            Some(home_code),
            Some(school_code),
            Some(workplace_code),
            Some(community_code),
        );
    }

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
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
    fn test_add_policy_time_trigger() {
        let mut context = setup();

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
        let policy: Policy<Alive, ItineraryModifier> = Policy {
            trigger: PolicyTrigger::TimeTrigger {
                start_time: 1.0,
                end_time: Some(5.0),
            },
            intervention: isolation_modifier,
            group: Alive(true),
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 1);
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.execute()
    }

    #[test]
    fn test_add_policy_hospitalization_event() {
        let mut context = setup();
        context.subscribe_to_hospitalization();
        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

        let policy: Policy<Alive, ItineraryModifier> = Policy {
            trigger: PolicyTrigger::HospitalizationThresholdTrigger { threshold: 1 },
            intervention: isolation_modifier,
            group: Alive(true),
        };

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Critical {
                    mild_time: 1.0,
                    severe_time: 1.0,
                    critical_time: 1.0,
                },
            );
            println!("{:?}", context.get_property::<Person, SymptomStatus>(p1));
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 1);
        });

        context.add_plan(2.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 1.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 2.0,
                },
            );
        });

        context.add_plan(2.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });
        context.execute()
    }

    #[test]
    fn test_add_policy_on_simulation_initialization() {
        let mut context = setup();

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

        let policy: Policy<SymptomStatus, ItineraryModifier> = Policy {
            trigger: PolicyTrigger::OnSimulationInitializationTrigger,
            intervention: isolation_modifier,
            group: SymptomStatus::Mild,
        };

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(2.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 1.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 2.0,
                },
            );
        });

        context.add_plan(2.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });
        context.execute()
    }
}
