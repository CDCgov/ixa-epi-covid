use ixa::prelude::*;

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

#[derive(Debug, Clone, Copy)]
pub struct TestStrategyProperties {
    pub test_type: TestType,
    pub testing_adherence: f64,
    pub testing_delay: f64,
    pub post_test_strategy: Option<PostTestStrategy>,
}

pub trait ContextTestStrategyExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextTestExt + ContextPostTestStrategyExt
{
    fn determine_if_testing_occurs(&mut self, testing_strategy_properties: TestStrategyProperties) -> bool {
        self.sample_bool(TestStrategyRng, testing_strategy_properties.testing_adherence)
    }

    fn conduct_test(&mut self, person_id: PersonId, test_strategy_properties: TestStrategyProperties) {
        if self.determine_if_testing_occurs(test_strategy_properties) {
            // Simulate testing delay
            let test_type = test_strategy_properties.test_type;
            let testing_delay = test_strategy_properties.testing_delay;
            let post_test_strategy = test_strategy_properties.post_test_strategy;

            self.add_plan(self.get_current_time() + testing_delay, move |context| {
                let result = context.test(test_type, person_id);
                if result && let Some(post_test_strategy) = post_test_strategy {
                    context.post_test_action(person_id, post_test_strategy);
                }
            });
        }
    }

    fn implement_test_strategy<P>(&mut self, group: P, test_strategy: TestStrategy)
    where        
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        match test_strategy {
            TestStrategy::Active(props) => self.implement_active_strategy(group, props),
            TestStrategy::Passive(props) => self.implement_passive_strategy(group, props),
        }
    }

    fn implement_active_strategy<P>(&mut self, group: P, test_strategy_properties: TestStrategyProperties)
    where        
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {   
        let people: Vec<_> = self.query_result_iterator(with!(Person, group)).collect();
        for person_id in people {
            self.conduct_test(person_id, test_strategy_properties);
        }
    }

    fn implement_passive_strategy<P>(&mut self, group: P, test_strategy_properties: TestStrategyProperties)
    where        
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        self.subscribe_to_event::<PropertyChangeEvent<Person, P>>(
            move |context, event| {
                if event.current == group {
                    context.conduct_test(event.entity_id, test_strategy_properties);
                }
            },
        );
    }


}
impl ContextTestStrategyExt for Context {}


