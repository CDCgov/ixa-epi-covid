use ixa::{Context, ContextGlobalPropertiesExt, IxaError, PluginContext, define_global_property};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Params {
    /// The random seed for the simulation.
    pub seed: u64,
    /// The maximum run time of the simulation; even if there are still infections
    /// scheduled to occur, the simulation will stop at this time.
    pub max_time: f64,
}

#[allow(clippy::too_many_lines)]
fn validate_inputs(parameters: &Params) -> Result<(), IxaError> {
    if parameters.max_time < 0.0 {
        return Err(IxaError::IxaError(
            "The max simulation running time must be non-negative.".to_string(),
        ));
    }
    Ok(())
}

define_global_property!(GlobalParams, Params, validate_inputs);

pub trait ParametersContextExt: PluginContext + ContextGlobalPropertiesExt {
    fn get_params(&self) -> &Params {
        self.get_global_property_value(GlobalParams)
            .expect("Expected GlobalParams to be set")
    }
}
impl ParametersContextExt for Context {}

impl Default for Params {
    fn default() -> Self {
        Params {
            seed: 0,
            max_time: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_validate_max_time() {
        let parameters = Params {
            max_time: -100.0,
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "The max simulation running time must be non-negative.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that the max simulation running time validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
}
