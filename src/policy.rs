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

        self.subscribe_to_event::<PolicyEvent>(move |context, event| {
            let group_property = policy.group;
            let intervention = policy.intervention;
            let policy_trigger_event = event.policy_trigger;
            if event.active && policy_trigger_event == policy_trigger {
                println!("{}", context.get_current_time());
                println!(
                    "Activating policy with group property {:?} and intervention {:?}",
                    group_property, intervention
                );
                intervention.activate(context, group_property);
            } else {
                println!(
                    "Deactivating policy with group property {:?} and intervention {:?}",
                    group_property, intervention
                );
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
        parameters::{GlobalParams, SettingProperties, TestProperties},
        pop_reader::{
            FIPSCode, PopulationReaderSettingCategory,
            parser::{parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id},
        },
        population_loader::{Alive, HomeId},
        settings::{PersonId, SettingCategory, SettingCode},
        surveillance::{
            test_manager::{Sensitivity, Test, TestAvailability, TestType, TestsConductedToday},
            test_strategy::{TestStrategy, TestStrategyProperties},
        },
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
            test_properties: vec![
                TestProperties {
                    test_type: TestType::PCR,
                    availability: TestAvailability::Unconstrained,
                    sensitivity: Sensitivity(1.0),
                },
                TestProperties {
                    test_type: TestType::Antigen,
                    availability: TestAvailability::Unconstrained,
                    sensitivity: Sensitivity(1.0),
                },
            ],
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        crate::surveillance::test_manager::init(&mut context);
        context
    }

    #[test]
    fn test_add_policy_active_test() {
        let mut context = setup();

        let test_strategy_properties = TestStrategyProperties {
            test_type: TestType::PCR,
            testing_adherence: 1.0,
            testing_delay: 0.0,
            post_test_strategy: None,
        };

        let test_strategy = TestStrategy::Active(test_strategy_properties);

        let policy_one: Policy<Alive, TestStrategy> = Policy {
            trigger: PolicyTrigger::TimeTrigger {
                start_time: 1.0,
                end_time: Some(2.0),
            },
            intervention: test_strategy,
            group: Alive(true),
        };

        let policy_two: Policy<Age, TestStrategy> = Policy {
            trigger: PolicyTrigger::TimeTrigger {
                start_time: 3.0,
                end_time: Some(4.0),
            },
            intervention: test_strategy,
            group: Age(30),
        };

        let policy_three: Policy<HomeId, TestStrategy> = Policy {
            trigger: PolicyTrigger::TimeTrigger {
                start_time: 5.0,
                end_time: Some(6.0),
            },
            intervention: test_strategy,
            group: HomeId(Some(make_home_id(b"160379602000010"))),
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        context.add_policy(policy_one);
        context.add_policy(policy_two);
        context.add_policy(policy_three);

        let pcr_test_id = context
            .query_result_iterator(with!(Test, test_strategy_properties.test_type))
            .next()
            .unwrap();
        // policy one
        context.add_plan(0.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                0
            );
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                2
            );
        });

        context.add_plan(2.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                2
            );
        });

        context.add_plan(3.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                3
            );
        });

        context.add_plan(4.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                3
            );
        });

        //policy three

        context.add_plan(5.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                5
            );
        });

        context.add_plan(6.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                5
            );
        });

        context.execute()
    }

    #[test]
    fn test_add_policy_passive_test() {
        let mut context = setup();

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();

        let test_strategy_properties = TestStrategyProperties {
            test_type: TestType::PCR,
            testing_adherence: 1.0,
            testing_delay: 0.0,
            post_test_strategy: None,
        };
        let test_strategy = TestStrategy::Passive(test_strategy_properties);

        let policy: Policy<SymptomStatus, TestStrategy> = Policy {
            trigger: PolicyTrigger::OnSimulationInitializationTrigger,
            intervention: test_strategy,
            group: SymptomStatus::Mild,
        };

        context.add_policy(policy);

        let pcr_test_id = context
            .query_result_iterator(with!(Test, test_strategy_properties.test_type))
            .next()
            .unwrap();

        context.add_plan(0.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                0
            );
        });

        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                1
            );
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
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                1
            );
        });
        context.execute()
    }
}
