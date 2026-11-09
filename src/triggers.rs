use ixa::{
    ExecutionPhase, IxaEvent,
    prelude::*,
    triggers::{ContextTriggersExt, PropertyValueCountTrigger, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};

use crate::{error::ModelError, infectiousness_manager::InfectionStatus};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum CustomTrigger {
    Time { start_time: f64, duration: f64 },
    InfectionPrevalence { threshold: usize },
}

impl CustomTrigger {
    pub fn validate(&self) -> Result<(), ModelError> {
        match self {
            Self::Time {
                start_time,
                duration,
            } => {
                if !start_time.is_finite() || *start_time < 0.0 {
                    return Err(ModelError::ModelError(
                        "start_time must be finite and non-negative".to_string(),
                    ));
                }
                if !duration.is_finite() || *duration <= 0.0 {
                    return Err(ModelError::ModelError(
                        "duration must be finite and positive".to_string(),
                    ));
                }
                Ok(())
            }
            Self::InfectionPrevalence { threshold } => {
                if *threshold == 0 {
                    Err(ModelError::ModelError(
                        "threshold must be positive".to_string(),
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
    pub fn check_overlap(self, other: CustomTrigger) -> bool {
        match (self, other) {
            (
                Self::Time {
                    start_time: start_a,
                    duration: dur_a,
                },
                Self::Time {
                    start_time: start_b,
                    duration: dur_b,
                },
            ) => {
                let end_a = start_a + dur_a;
                let end_b = start_b + dur_b;
                start_a < end_b && start_b < end_a
            }
            _ => false,
        }
    }
}

impl PartialEq for CustomTrigger {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Time {
                    start_time: left_start,
                    duration: left_duration,
                },
                Self::Time {
                    start_time: right_start,
                    duration: right_duration,
                },
            ) => left_start == right_start && left_duration == right_duration,
            (
                Self::InfectionPrevalence {
                    threshold: left_threshold,
                },
                Self::InfectionPrevalence {
                    threshold: right_threshold,
                },
            ) => left_threshold == right_threshold,
            _ => false,
        }
    }
}

pub trait ContextCustomTriggerExt: PluginContext + ContextEntitiesExt + ContextTriggersExt {
    fn set_custom_trigger<E, F>(
        &mut self,
        trigger: CustomTrigger,
        starting_event: E,
        ending_event: F,
    ) where
        E: IxaEvent,
        F: IxaEvent,
    {
        match trigger {
            CustomTrigger::Time {
                start_time,
                duration,
            } => {
                let start_trigger = TimeTrigger::at_phase(start_time, ExecutionPhase::Last)
                    .emit_value(starting_event);
                self.register_trigger(start_trigger);
                let end_trigger =
                    TimeTrigger::at_phase(start_time + duration, ExecutionPhase::Last)
                        .emit_value(ending_event);
                self.register_trigger(end_trigger);
            }
            CustomTrigger::InfectionPrevalence { threshold } => {
                let start_trigger =
                    PropertyValueCountTrigger::increases_to(InfectionStatus::Infectious, threshold)
                        .emit_value(starting_event);
                self.register_trigger(start_trigger);
                let end_trigger =
                    PropertyValueCountTrigger::decreases_to(InfectionStatus::Infectious, threshold)
                        .emit_value(ending_event);
                self.register_trigger(end_trigger);
            }
        }
    }
}
impl ContextCustomTriggerExt for Context {}
