use std::any::Any;

use ixa::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    intervention_manager::{InterventionModifierSpecification, Modifier, ModifierSpecification},
    settings::Person,
};

pub mod symptom_test_guidance;
pub mod symptom_time_guidance;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct EligibleForIsolationStatus(pub bool);
impl_property!(
    EligibleForIsolationStatus,
    Person,
    default_const = EligibleForIsolationStatus(false)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct SymptomTimeGuidance {
    // the duration of isolation required after symptoms have resolved
    pub post_symptoms_isolation_duration: f64,
    // the minimum duration of isolation regardless of symptom resolution
    pub minimum_isolation_duration: f64,
    // the delay from symptom onset to starting isolation. This is is not a
    // parameter of the guidance itself, but rather a modeling accuracy consideration
    pub delay_symptoms_to_isolation: f64,
}

impl SymptomTimeGuidance {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.post_symptoms_isolation_duration < 0.0 {
            return Err("post_symptoms_isolation_duration must be nonnegative");
        }
        if self.minimum_isolation_duration < 0.0 {
            return Err("minimum_isolation_duration must be nonnegative");
        }
        if self.delay_symptoms_to_isolation < 0.0 {
            return Err("delay_symptoms_to_isolation must be nonnegative");
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct SymptomTestGuidance {
    // the number of negative test required to end isoilation
    pub tests_post_symptoms: usize,
    // interval between tests
    pub test_interval: f64,
    // test sensitivity
    pub test_sensitivity: f64,
    // the delay from test to starting isolation. This is not a
    // parameter of the guidance itself, but rather a modeling accuracy consideration
    pub delay_symptoms_to_isolation: f64,
}

impl SymptomTestGuidance {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.test_interval <= 0.0 {
            return Err("test_interval greater than zero");
        }
        if !(0.0..=1.0).contains(&self.test_sensitivity) {
            return Err("test_sensitivity must be between zero and one, inclusive");
        }
        if self.delay_symptoms_to_isolation < 0.0 {
            return Err("delay_symptoms_to_isolation must benonnegative");
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum IsolationGuidanceParameters {
    SymptomTimeGuidance(SymptomTimeGuidance),
    SymptomTestGuidance(SymptomTestGuidance),
}

impl IsolationGuidanceParameters {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::SymptomTimeGuidance(guidance) => guidance.validate(),
            Self::SymptomTestGuidance(guidance) => guidance.validate(),
        }
    }
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
    let Params {
        interventions,
        default_modifiers,
        ..
    } = context.get_params();

    let default_isolation_guidance = default_modifiers.get(&Modifier::IsolationGuidance);
    let activate_isolation_guidance = interventions
        .iter()
        .any(|i| i.modifier == Modifier::IsolationGuidance);
    if let Some(default_isolation_guidance) = default_isolation_guidance
        && activate_isolation_guidance
        && let InterventionModifierSpecification::IsolationGuidance {
            isolation_guidance, ..
        } = default_isolation_guidance
    {
        match isolation_guidance {
            IsolationGuidanceParameters::SymptomTimeGuidance(_symptom_time_guidance) => {
                symptom_time_guidance::init(context)?;
            }
            IsolationGuidanceParameters::SymptomTestGuidance(_symptom_test_guidance) => {
                symptom_test_guidance::init(context)?;
            }
        }
    }
    Ok(())
}
