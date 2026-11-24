use std::any::Any;

use ixa::prelude::*;
use serde::Serialize;

use crate::{error::ModelError, intervention_manager::ModifierSpecification, settings::Person};

pub mod time_based_isolation_guidance;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct EligibleForIsolation(pub bool);
impl_property!(
    EligibleForIsolation,
    Person,
    default_const = EligibleForIsolation(false)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub struct TimeBasedIsolationGuidance {
    // the duration of isolation required after symptoms have resolved
    pub post_symptoms_isolation_duration: f64,
    // the minimum duration of isolation regardless of symptom resolution
    pub minimum_isolation_duration: f64,
    // the delay from symptom onset to starting isolation. This is is not a
    // parameter of the guidance itself, but rather a modeling realism consideration
    pub delay_symptoms_to_isolation: f64,
}

pub struct IsolationGuidanceModifiers {
    pub itinerary_modifier: Box<dyn ModifierSpecification>,
    pub transmission_modifier: Box<dyn ModifierSpecification>,
}

impl ModifierSpecification for IsolationGuidanceModifiers {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    time_based_isolation_guidance::init(context)?;
    Ok(())
}
