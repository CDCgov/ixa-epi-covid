use ixa::prelude::*;

use crate::population_loader::PersonId;
use crate::surveillance::{
    post_test_strategy::{ContextPostTestStrategyExt, PostTestStrategy},
    test_manager::{ContextTestExt, TestType},
};

define_rng!(TestStrategyRng);

pub trait TestTrait {
    fn determine_if_testing_occurs(&mut self) -> bool;
    fn conduct_test(&mut self, context: &mut Context, person_id: PersonId);
}

#[derive(Debug, Clone)]
pub struct TestStrategy {
    test_type: TestType,
    testing_adherence: f64,
    testing_delay: f64,
    post_test_strategy: Option<PostTestStrategy>,
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

pub trait ContextTestStrategyExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextTestExt + ContextPostTestStrategyExt
{
    fn conduct_test(&mut self, person_id: PersonId, test_strategy: TestStrategy);
}
impl ContextTestStrategyExt for Context {
    fn conduct_test(&mut self, person_id: PersonId, mut test_strategy: TestStrategy) {
        if test_strategy.determine_if_testing_occurs() {
            test_strategy.conduct_test(self, person_id);
        }
    }
}
