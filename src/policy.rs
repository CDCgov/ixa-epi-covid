use ixa::prelude::*;
use serde::Serialize;

use crate::{
    ContextParametersExt,
    settings::{ContextSettingExt, Person},
    surveillance::test_strategy::ContextTestStrategyExt,
};

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct TimeTrigger {
    start_time: f64,
    end_time: f64,
}

pub trait InterventionTrait: std::fmt::Debug + Copy + 'static {
    fn exogenous_activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
    fn exogenous_deactivate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
    fn endogenous_activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ExogenousPolicy<P, I>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    I: InterventionTrait,
{
    trigger: TimeTrigger,
    intervention: I,
    group: P,
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct EndogenousPolicy<P, I>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    I: InterventionTrait,
{
    intervention: I,
    group: P,
}

pub trait ContextPolicyExt:
    PluginContext
    + ContextEntitiesExt
    + ContextParametersExt
    + ContextSettingExt
    + ContextTestStrategyExt
{
    fn add_exogenous_policy<P, I>(&mut self, policy: ExogenousPolicy<P, I>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        I: InterventionTrait,
    {
        let start_time = policy.trigger.start_time;
        let end_time = policy.trigger.end_time;
        let group_property = policy.group;
        let intervention = policy.intervention;

        self.add_plan(start_time, move |context| {
            intervention.exogenous_activate(context, group_property);
        });
        self.add_plan(end_time, move |context| {
            intervention.exogenous_deactivate(context, group_property);
        });
    }

    fn add_endogenous_policy<P, I>(&mut self, policy: EndogenousPolicy<P, I>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        I: InterventionTrait;
}
impl ContextPolicyExt for Context {
    fn add_endogenous_policy<P, I>(&mut self, policy: EndogenousPolicy<P, I>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        I: InterventionTrait,
    {
        let group_property = policy.group;
        let intervention = policy.intervention;
        intervention.endogenous_activate(self, group_property);
    }
}

#[cfg(test)]
mod tests {
    use ixa::{Context, HashMap};

    use crate::{
        Age, Params,
        parameters::{GlobalParams, SettingProperties, TestProperties},
        population_loader::Alive,
        settings::SettingCategory,
        surveillance::{
            test_manager::{Sensitivity, Test, TestAvailability, TestType, TestsConductedToday},
            test_strategy::TestStrategy,
        },
    };

    use super::*;

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
    fn test_add_policy_population() {
        let mut context = setup();

        let test_strategy = TestStrategy {
            test_type: TestType::PCR,
            testing_adherence: 1.0,
            testing_delay: 0.0,
            post_test_strategy: None,
        };

        let policy: ExogenousPolicy<Alive, TestStrategy> = ExogenousPolicy {
            trigger: TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            },
            intervention: test_strategy,
            group: Alive(true),
        };

        let _p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let _p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        context.add_exogenous_policy(policy);

        let pcr_test_id = context
            .query_result_iterator(with!(Test, test_strategy.test_type))
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

        context.add_plan(1.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                2
            );
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(
                context
                    .get_property::<Test, TestsConductedToday>(pcr_test_id)
                    .0,
                2
            );
        });
        context.execute()
    }

    // #[test]
    // fn test_add_policy_query() {
    //     let mut context = setup();

    //     let isolation_matrix = [
    //         [0.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //         [1.0, 0.0, 0.0, 0.0],
    //     ];

    //     let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

    //     let policy: ExogenousPolicy<Age> = ExogenousPolicy {
    //         trigger: Trigger::Time(TimeTrigger {
    //             start_time: 1.0,
    //             end_time: 5.0,
    //         }),
    //         intervention: Intervention::Isolation(isolation_modifier),
    //         group: Age(30),
    //     };

    //     let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
    //     let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

    //     add_person_to_test_settings(&mut context, p1);
    //     add_person_to_test_settings(&mut context, p2);

    //     // Policies must be added after population is added
    //     context.add_exogenous_policy(policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(5.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });
    //     context.execute()
    // }

    // #[test]
    // fn test_add_policy_setting_code() {
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
    //     let policy: ExogenousPolicy<HomeId> = ExogenousPolicy {
    //         trigger: Trigger::Time(TimeTrigger {
    //             start_time: 1.0,
    //             end_time: 5.0,
    //         }),
    //         intervention: Intervention::Isolation(isolation_modifier),
    //         group: HomeId(Some(make_home_id(b"160379602000010"))),
    //     };

    //     context.add_exogenous_policy(policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 1);
    //     });

    //     context.add_plan(5.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });
    //     context.execute()
    // }

    // #[test]
    // fn test_endogenous_policy() {
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

    //     let individual_policy: EndogenousPolicy<SymptomStatus> = EndogenousPolicy {
    //         intervention: Intervention::Isolation(isolation_modifier),
    //         group: SymptomStatus::Mild,
    //     };

    //     context.add_endogenous_policy(individual_policy);

    //     context.add_plan(0.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(1.0, move |context| {
    //         context.set_property(p1, SymptomStatus::Mild);
    //     });

    //     context.add_plan(1.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });

    //     context.add_plan(2.0, move |context| {
    //         context.set_property(p1, SymptomStatus::Resolved);
    //     });

    //     context.add_plan(2.5, move |context| {
    //         assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
    //         assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
    //     });
    // }
}
