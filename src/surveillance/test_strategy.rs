use ixa::prelude::*;

use crate::{surveillance::{test_manager::{ContextTestExt, TestType}}};
use crate::{
    population_loader::{PersonId},
};

define_rng!(TestStrategyRng);

pub trait TestTrait {}

pub struct TestStrategy {
    test_type: TestType,
    testing_adherence: f64,
    testing_delay: f64,
}

impl TestTrait for TestStrategy {}

pub trait ContextTestStrategyExt: PluginContext + ContextEntitiesExt + ContextRandomExt + ContextTestExt{
    fn conduct_test(&mut self, person_id: PersonId, test_strategy: TestStrategy) {
        if self.sample_bool(TestStrategyRng, test_strategy.testing_adherence) {
            // Simulate testing delay
            self.add_plan(
                self.get_current_time() + test_strategy.testing_delay,
                move |context| {
                    let _result = context.test(test_strategy.test_type, person_id);
                },
            );
        }
    }
}
impl ContextTestStrategyExt for Context {}