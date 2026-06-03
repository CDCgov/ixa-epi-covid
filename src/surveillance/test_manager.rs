use ixa::prelude::*;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;

use crate::{
    ContextParametersExt,
    infectiousness_manager::InfectionStatus,
    settings::{Person, PersonId},
};

define_rng!(TestRng);

define_entity!(Test);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub enum TestType {
    PCR,
    Antigen,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sensitivity(pub f64);

impl PartialEq for Sensitivity {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Sensitivity {}

impl std::hash::Hash for Sensitivity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub enum TestAvailability {
    Unconstrained,
    MaxPerDay(usize),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub struct TestsConductedToday(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub struct PositiveTestsConductedToday(pub usize);

impl_property!(TestType, Test);
impl_property!(Sensitivity, Test);
impl_property!(TestAvailability, Test);
impl_property!(
    TestsConductedToday,
    Test,
    default_const = TestsConductedToday(0)
);
impl_property!(
    PositiveTestsConductedToday,
    Test,
    default_const = PositiveTestsConductedToday(0)
);

#[allow(dead_code)]
pub trait ContextTestExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextParametersExt
{
    fn load_test_entities(&mut self) {
        let test_properties = self.get_params().test_properties.clone();
        for test_property in test_properties {
            let _test_entity = self
                .add_entity(with!(
                    Test,
                    test_property.test_type,
                    test_property.sensitivity,
                    test_property.availability
                ))
                .unwrap();
        }
    }

    fn check_test_availability(&self, test: TestId) -> bool {
        let availability = self.get_property::<Test, TestAvailability>(test);
        match availability {
            TestAvailability::Unconstrained => true,
            TestAvailability::MaxPerDay(max) => {
                let tests_conducted_today = self.get_property::<Test, TestsConductedToday>(test).0;
                tests_conducted_today < max
            }
        }
    }

    fn increment_tests_conducted(&mut self, test: TestId) {
        let mut tests_conducted_today = self.get_property::<Test, TestsConductedToday>(test).0;
        tests_conducted_today += 1;
        self.set_property::<Test, TestsConductedToday>(
            test,
            TestsConductedToday(tests_conducted_today),
        );
    }

    fn increment_positive_tests_conducted(&mut self, test: TestId) {
        let mut positive_tests_conducted_today = self
            .get_property::<Test, PositiveTestsConductedToday>(test)
            .0;
        positive_tests_conducted_today += 1;
        self.set_property::<Test, PositiveTestsConductedToday>(
            test,
            PositiveTestsConductedToday(positive_tests_conducted_today),
        );
    }

    fn test(&mut self, test_type: TestType, person_id: PersonId) -> bool {
        // the strategy type and test type together uniquely identify the test to be conducted, so we can query for the test id using both properties
        let test_id = self
            .query_result_iterator(with!(Test, test_type))
            .next()
            .unwrap();
        let infectious_status = self.get_property::<Person, InfectionStatus>(person_id);
        let test_sensitivity = self.get_property::<Test, Sensitivity>(test_id).0;
        if !self.check_test_availability(test_id) {
            return false;
        }
        self.increment_tests_conducted(test_id);
        match infectious_status {
            InfectionStatus::Infectious => {
                let positive = self.sample_bool(TestRng, test_sensitivity);
                if positive {
                    self.increment_positive_tests_conducted(test_id);
                }
                positive
            }
            _ => false,
        }
    }
}
impl ContextTestExt for Context {}

pub fn init(context: &mut Context) {
    context.load_test_entities();
    context.index_property::<Test, TestType>();
}
