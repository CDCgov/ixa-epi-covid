use ixa::prelude::*;

use crate::policy::InterventionTrait;
use crate::population_loader::PersonId;
use crate::settings::Person;
use crate::surveillance::{
    post_test_strategy::{ContextPostTestStrategyExt, PostTestStrategy},
    test_manager::{ContextTestExt, TestType},
};

define_rng!(TestStrategyRng);

#[derive(Debug, Clone, Copy)]
pub enum TestStrategy {
    Active(TestStrategyProperties),
    Passive(TestStrategyProperties),
}

pub trait TestTrait {
    fn determine_if_testing_occurs(&mut self) -> bool;
    fn conduct_test(&mut self, context: &mut Context, person_id: PersonId);
}

#[derive(Debug, Clone, Copy)]
pub struct TestStrategyProperties {
    pub test_type: TestType,
    pub testing_adherence: f64,
    pub testing_delay: f64,
    pub post_test_strategy: Option<PostTestStrategy>,
}

// this test strategy is used when people need to be queried and tested.
impl TestTrait for TestStrategyProperties {
    fn determine_if_testing_occurs(&mut self) -> bool {
        self.testing_adherence > 0.0
    }

    // it would be nice to disentangle the testing and post testing triggering
    fn conduct_test(&mut self, context: &mut Context, person_id: PersonId) {
        if context.sample_bool(TestStrategyRng, self.testing_adherence) {
            // Simulate testing delay
            let test_type = self.test_type;
            let testing_delay = self.testing_delay;
            let post_test_strategy = self.post_test_strategy;
            context.add_plan(context.get_current_time() + testing_delay, move |context| {
                let result = context.test(test_type, person_id);
                if result && let Some(post_test_strategy) = post_test_strategy {
                    context.post_test_action(person_id, post_test_strategy);
                }
            });
        }
    }
}

impl InterventionTrait for TestStrategy {
    fn activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let strategy = *self;
        match strategy {
            TestStrategy::Active(strategy) => {
                let person_ids: Vec<_> = context
                    .query_result_iterator(with!(Person, group_property))
                    .collect();
                for person_id in person_ids {
                    context.conduct_test(person_id, strategy);
                }
            }
            TestStrategy::Passive(strategy) => {
                context.subscribe_to_event::<PropertyChangeEvent<Person, P>>(
                    move |context, event| {
                        if event.current == group_property {
                            context.conduct_test(event.entity_id, strategy);
                        }
                    },
                );
            }
        }
    }
    fn deactivate<P>(&self, _context: &mut Context, _group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
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
