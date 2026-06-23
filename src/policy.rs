use ixa::{HashMap, prelude::*, prelude_for_plugins::IxaEvent, triggers::{ContextTriggersExt, TriggerSpec}};
use serde::Serialize;

use crate::{
    ContextParametersExt,
    settings::{ContextSettingExt, Person},
};

#[derive(IxaEvent, Debug)]
pub struct PolicyEvent
{
    active: bool,
    trigger_name: &'static str,
}

pub trait InterventionTrait: std::fmt::Debug + Copy + 'static {
    fn activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug + std::hash::Hash + Eq;
    fn deactivate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug + std::hash::Hash + Eq;
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct Policy<P, I, T>
where
    P: Property<Person> + std::fmt::Debug + std::hash::Hash + Eq,
    I: InterventionTrait,
    T: TriggerSpec,
{
    pub trigger: T,
    pub intervention: I,
    pub group: P,
}

pub trait ContextPolicyExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt + ContextTriggersExt
{
    fn add_policy<P, I, T>(&mut self, policy: Policy<P, I, T>)
    where
        P: Property<Person> + std::fmt::Debug + std::hash::Hash + Eq,
        I: InterventionTrait,
        T: TriggerSpec,
    {
        let policy_trigger = policy.trigger;
        let group_property = policy.group;
        let intervention = policy.intervention;

        self.register_trigger(policy_trigger);


        self.subscribe_to_event::<PolicyEvent>(move |context, event| {

            if event.active  {
                intervention.activate(context, group_property);
            } else {
                intervention.deactivate(context, group_property);
            }
        });
    }
}
impl ContextPolicyExt for Context {}

#[cfg(test)]
mod tests {
    use ixa::{Context, ExecutionPhase, HashMap, triggers::{TimeTrigger, TogglingTriggerCriteria}};

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
        symptom_status_manager::{SymptomStatus},
    };

    define_multi_property!((Age, SymptomStatus), Person);

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

        let trigger = TogglingTriggerCriteria::new(
            TimeTrigger::at_phase(
                1.0,
                ExecutionPhase::Last,
            ),
            TimeTrigger::at_phase(
                5.0,
                ExecutionPhase::Last,
            ),
        ).emit_values(PolicyEvent{active: true, trigger_name: "TimeTrigger"}, PolicyEvent{active: false, trigger_name: "TimeTrigger"});
        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
        let policy: Policy<Alive, ItineraryModifier, _> = Policy {
            trigger,
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

    // #[test]
    // fn test_add_policy_hospitalization_event() {
    //     let mut context = setup();
    //     context.subscribe_to_hospitalization();
    //     let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
    //     let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

    //     add_person_to_test_settings(&mut context, p1);
    //     add_person_to_test_settings(&mut context, p2);

    //     let isolation_matrix = [
    //         [0.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //     ];

    //     let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

    //     let policy: Policy<Alive, ItineraryModifier> = Policy {
    //         trigger: PolicyTrigger::HospitalizationThresholdTrigger { threshold: 1 },
    //         intervention: isolation_modifier,
    //         group: Alive(true),
    //     };

    //     context.add_policy(policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.0, move |context| {
    //         context.set_property::<Person, SymptomData>(
    //             p1,
    //             SymptomData::Critical {
    //                 mild_time: 1.0,
    //                 severe_time: 1.0,
    //                 critical_time: 1.0,
    //             },
    //         );
    //         println!("{:?}", context.get_property::<Person, SymptomStatus>(p1));
    //     });

    //     context.add_plan(1.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 1);
    //     });

    //     context.add_plan(2.0, move |context| {
    //         context.set_property::<Person, SymptomData>(
    //             p1,
    //             SymptomData::Resolved {
    //                 mild_time: 1.0,
    //                 severe_time: None,
    //                 critical_time: None,
    //                 resolved_time: 2.0,
    //             },
    //         );
    //     });

    //     context.add_plan(2.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });
    //     context.execute()
    // }

    // #[test]
    // fn test_add_policy_on_simulation_initialization() {
    //     let mut context = setup();

    //     let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
    //     let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

    //     add_person_to_test_settings(&mut context, p1);
    //     add_person_to_test_settings(&mut context, p2);

    //     let isolation_matrix = [
    //         [0.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //     ];

    //     let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

    //     let policy: Policy<SymptomStatus, ItineraryModifier> = Policy {
    //         trigger: PolicyTrigger::OnSimulationInitializationTrigger,
    //         intervention: isolation_modifier,
    //         group: SymptomStatus::Mild,
    //     };

    //     context.add_policy(policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.0, move |context| {
    //         context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
    //     });

    //     context.add_plan(1.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(2.0, move |context| {
    //         context.set_property::<Person, SymptomData>(
    //             p1,
    //             SymptomData::Resolved {
    //                 mild_time: 1.0,
    //                 severe_time: None,
    //                 critical_time: None,
    //                 resolved_time: 2.0,
    //             },
    //         );
    //     });

    //     context.add_plan(2.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });
    //     context.execute()
    // }

    // #[test]
    // fn test_add_policy_periodic_time_trigger() {
    //     let mut context = setup();
    //     context.set_start_time(0.0);
    //     let weekend_matrix = [
    //         [0.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //     ];

    //     let weekend_modifier = define_itinerary_modifier(Some(weekend_matrix), None);
    //     let policy: Policy<Age, ItineraryModifier> = Policy {
    //         trigger: PolicyTrigger::PeriodicTimeTrigger {
    //             interval: 7.0,
    //             duration: 2.0,
    //             start_time: 1.0,
    //             end_time: 15.0,
    //         },
    //         intervention: weekend_modifier,
    //         group: Age(30),
    //     };

    //     let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
    //     let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

    //     add_person_to_test_settings(&mut context, p1);
    //     add_person_to_test_settings(&mut context, p2);

    //     context.add_policy(policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(2.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(3.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(4.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(6.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(8.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(10.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.execute()
    // }

    // #[test]
    // fn test_add_policy_with_multi_property() {
    //     let mut context = setup();

    //     let isolation_matrix = [
    //         [0.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //     ];

    //     let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
    //     let policy: Policy<(Age, SymptomStatus), ItineraryModifier> = Policy {
    //         trigger: PolicyTrigger::TimeTrigger {
    //             start_time: 1.0,
    //             end_time: Some(5.0),
    //         },
    //         intervention: isolation_modifier,
    //         group: (Age(30), SymptomStatus::Mild),
    //     };

    //     let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
    //     let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

    //     add_person_to_test_settings(&mut context, p1);
    //     add_person_to_test_settings(&mut context, p2);

    //     context.add_policy(policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.0, move |context| {
    //         context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
    //     });

    //     context.add_plan(1.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(2.0, move |context| {
    //         context.set_property::<Person, SymptomData>(
    //             p1,
    //             SymptomData::Resolved {
    //                 mild_time: 1.0,
    //                 severe_time: None,
    //                 critical_time: None,
    //                 resolved_time: 2.0,
    //             },
    //         );
    //     });

    //     context.add_plan(2.0, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(5.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.execute()
    // }
}
