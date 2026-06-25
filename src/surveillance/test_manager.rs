use ixa::{ExecutionPhase, HashMap, prelude::*, prelude_for_plugins::IxaEvent};
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt,
    infectiousness_manager::InfectionStatus,
    settings::{Person, PersonId},
};

define_rng!(TestRng);

#[derive(IxaEvent, Copy, Clone, Debug)]
#[allow(dead_code)]
pub struct TestEvent {
    result: bool,
    person_id: PersonId,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub enum TestType {
    PCR,
    Antigen,
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum TestAvailability {
    Unconstrained,
    MaxPerDay { limit: usize },
}

#[derive(Default)]
struct TestData {
    test_availability: HashMap<TestType, TestAvailability>,
    test_conducted_per_day: HashMap<TestType, usize>,
    test_sensitivity: HashMap<TestType, f64>,
}

impl TestData {
    fn get_test_availability(&self, test_type: TestType) -> TestAvailability {
        *self
            .test_availability
            .get(&test_type)
            .unwrap_or(&TestAvailability::Unconstrained)
    }

    fn set_test_availability(&mut self, test_type: TestType, availability: TestAvailability) {
        self.test_availability.insert(test_type, availability);
    }

    fn get_tests_conducted_per_day(&self, test_type: TestType) -> usize {
        *self.test_conducted_per_day.get(&test_type).unwrap_or(&0)
    }

    fn increment_tests_conducted_per_day(&mut self, test_type: TestType) {
        let count = self.test_conducted_per_day.entry(test_type).or_insert(0);
        *count += 1;
    }

    fn get_test_sensitivity(&self, test_type: TestType) -> f64 {
        *self.test_sensitivity.get(&test_type).unwrap_or(&1.0)
    }

    fn set_test_sensitivity(&mut self, test_type: TestType, sensitivity: f64) {
        self.test_sensitivity.insert(test_type, sensitivity);
    }

    fn reset_daily_counts(&mut self, test_type: TestType) {
        if let Some(count) = self.test_conducted_per_day.get_mut(&test_type) {
            *count = 0;
        }
    }
}

define_data_plugin!(TestDataPlugin, TestData, TestData::default());

#[allow(dead_code)]
pub trait ContextTestExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextParametersExt
{
    fn emit_test_event(&mut self, result: bool, person_id: PersonId) {
        self.emit_event(TestEvent { result, person_id });
    }

    fn load_test_data(&mut self) {
        let test_properties = self.get_params().test_properties.clone();
        let data = self.get_data_mut(TestDataPlugin);
        for test_property in test_properties {
            data.set_test_availability(test_property.test_type, test_property.availability);
            data.set_test_sensitivity(test_property.test_type, test_property.sensitivity);
        }
    }

    fn check_test_availability(&self, test: TestType) -> bool {
        let test_data = self.get_data(TestDataPlugin);
        let availability = test_data.get_test_availability(test);
        match availability {
            TestAvailability::Unconstrained => true,
            TestAvailability::MaxPerDay { limit } => {
                let tests_conducted_per_day = test_data.get_tests_conducted_per_day(test);
                tests_conducted_per_day < limit
            }
        }
    }

    fn increment_tests_conducted(&mut self, test: TestType) {
        let data = self.get_data_mut(TestDataPlugin);
        data.increment_tests_conducted_per_day(test);
    }

    fn test(&mut self, test_type: TestType, person_id: PersonId) -> bool {
        let infectious_status = self.get_property::<Person, InfectionStatus>(person_id);
        let test_data = self.get_data(TestDataPlugin);
        let test_sensitivity = test_data.get_test_sensitivity(test_type);
        if !self.check_test_availability(test_type) {
            return false;
        }
        self.increment_tests_conducted(test_type);
        match infectious_status {
            InfectionStatus::Infectious => {
                let positive = self.sample_bool(TestRng, test_sensitivity);
                self.emit_test_event(positive, person_id);
                positive
            }
            _ => {
                self.emit_test_event(false, person_id);
                false
            }
        }
    }

    fn get_tests_conducted_per_day(&self, test_type: TestType) -> usize {
        let test_data = self.get_data(TestDataPlugin);
        test_data.get_tests_conducted_per_day(test_type)
    }

    fn schedule_daily_reset(&mut self) {
        let test_types: Vec<_> = self
            .get_data(TestDataPlugin)
            .test_availability
            .keys()
            .copied()
            .collect();

        for test_type in test_types {
            self.add_periodic_plan_with_phase(
                1.0,
                move |context: &mut Context| {
                    context
                        .get_data_mut(TestDataPlugin)
                        .reset_daily_counts(test_type);
                },
                ExecutionPhase::Last,
            );
        }
    }
}
impl ContextTestExt for Context {}

pub fn init(context: &mut Context) {
    context.load_test_data();
}
