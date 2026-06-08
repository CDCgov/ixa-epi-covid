use ixa::prelude::*;

use crate::population_loader::PersonId;
use crate::settings::SETTING_COUNT;
use crate::surveillance::test_manager::{ContextTestExt, TestType};

define_rng!(PostTestStrategyRng);
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct ContactTracingStrategy {
    test_type: TestType,            // Type of test to be taken after being identified
    contact_tracing_adherence: f64, // probability someone contact traces
    testing_delay: f64,             // delay between being identified as a contact and taking a test
    testing_adherence: f64, // probability that someone who is identified as a contact will actually test
    setting_contact_tracing: [f64; SETTING_COUNT], // setting-specific probabilities of contact tracing, indexed by setting type
}

#[derive(Debug, Clone, Copy)]
pub struct PostTestStrategy {
    contact_tracing_strategy: Option<ContactTracingStrategy>, // contact tracing strategy to be implemented after testing positive
    itinerary_modification: Option<bool>, // whether or not a person modifies their itinerary after testing positive
}

pub trait ContextPostTestStrategyExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextTestExt
{
    fn post_test_action(&mut self, person_id: PersonId, post_test_strategy: PostTestStrategy) {
        if let Some(itinerary_modification) = post_test_strategy.itinerary_modification
            && itinerary_modification
        {
            println!(
                "Implementing itinerary modification for person {}",
                person_id
            );
        }
        // Implement itinerary modification logic here, e.g., self.modify_itinerary(person_id);
        if let Some(contact_tracing_strategy) = post_test_strategy.contact_tracing_strategy {
            println!(
                "Implementing contact tracing strategy for person {}: {:?}",
                person_id, contact_tracing_strategy
            );
        }
    }
    fn initialize_post_test_strategy(&mut self, post_test_strategy: PostTestStrategy) {
        println!(
            "Initializing post-test strategy for the model: {:?}",
            post_test_strategy
        );
    }
}
impl ContextPostTestStrategyExt for Context {}
