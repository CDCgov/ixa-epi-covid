use ixa::prelude::*;

use crate::policy::InterventionTrait;
use crate::population_loader::PersonId;
use crate::settings::Person;
use crate::surveillance::{
    post_test_strategy::{ContextPostTestStrategyExt, PostTestStrategy},
    test_manager::{ContextTestExt, TestType},
};

define_rng!(TestStrategyRng);

pub trait TestTrait {
    fn determine_if_testing_occurs(&mut self) -> bool;
    fn conduct_test(&mut self, context: &mut Context, person_id: PersonId);
}

#[derive(Debug, Clone, Copy)]
pub struct TestStrategy {
    pub test_type: TestType,
    pub testing_adherence: f64,
    pub testing_delay: f64,
    pub post_test_strategy: Option<PostTestStrategy>,
}

impl TestTrait for TestStrategy {
    fn determine_if_testing_occurs(&mut self) -> bool {
        self.testing_adherence > 0.0
    }

    // it would be nice to disentangle the testing and post testing triggering
    fn conduct_test(&mut self, context: &mut Context, person_id: PersonId) {
        if context.sample_bool(TestStrategyRng, self.testing_adherence) {
            // Simulate testing delay
            let test_type = self.test_type;
            let testing_delay = self.testing_delay;
            let post_test_strategy = self.post_test_strategy.clone();
            context.add_plan(context.get_current_time() + testing_delay, move |context| {
                let result = context.test(test_type, person_id);
                if result {
                    if let Some(post_test_strategy) = post_test_strategy.clone() {
                        context.post_test_action(person_id, post_test_strategy);
                    }
                }
            });
        }
    }
}

impl InterventionTrait for TestStrategy {
    fn endogenous_activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let strategy = *self;
        context.subscribe_to_event::<PropertyChangeEvent<Person, P>>(move |context, event| {
            if event.current == group_property {
                context.conduct_test(event.entity_id, strategy);
            }
        });
    }

    fn exogenous_activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let strategy = *self;

        let person_ids: Vec<_> = context
            .query_result_iterator(with!(Person, group_property))
            .collect();
        for person_id in person_ids {
            context.conduct_test(person_id, strategy);
        }
    }

    fn exogenous_deactivate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        println!(
            "Deactivating test strategy intervention for group {:?} with strategy {:?}",
            group_property, self
        );
        println!("{}", context.get_current_time());
    }
}

pub trait ContextTestStrategyExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextTestExt + ContextPostTestStrategyExt
{
    fn conduct_test<T: TestTrait>(&mut self, person_id: PersonId, test_strategy: T);
}
impl ContextTestStrategyExt for Context {
    fn conduct_test<T: TestTrait>(&mut self, person_id: PersonId, mut test_strategy: T) {
        if test_strategy.determine_if_testing_occurs() {
            test_strategy.conduct_test(self, person_id);
        }
    }
}
