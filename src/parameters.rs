use ixa::{HashMap, HashMapExt, prelude::*};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, path::PathBuf};

use crate::reports::ReportParams;
use crate::settings::SettingProperties;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RateFnType {
    /// A constant rate of infectiousness (constant hazard -> exponential waiting times) for a given
    /// duration.
    Constant { rate: f64, duration: f64 },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub enum CoreSettingsTypes {
    Home,
    School,
    Workplace,
    CensusTract,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Params {
    /// The random seed for the simulation.
    pub seed: u64,
    /// The maximum run time of the simulation; even if there are still infections
    /// scheduled to occur, the simulation will stop at this time.
    pub max_time: f64,
    /// The path to the synthetic population file loaded in `population_loader`
    pub synth_population_file: PathBuf,
    /// The proportion of initial people who are infectious when we seed the population.
    pub initial_incidence: f64,
    /// A library of infection rates to assign to infected people.
    pub infectiousness_rate_fn: RateFnType,
    /// Setting properties by setting type
    pub settings_properties: HashMap<CoreSettingsTypes, SettingProperties>,
    /// ratios used to initialize indiviiduals itineraries by setting type.
    pub itinerary_ratios: HashMap<CoreSettingsTypes, f64>,
    /// Prevalence report with a period and name required
    pub prevalence_report: ReportParams,
    /// Incidence report with a period and name required
    pub incidence_report: ReportParams,
    /// Transmission report with a name required
    pub transmission_report: ReportParams,
}

#[allow(clippy::too_many_lines)]
fn validate_inputs(parameters: &Params) -> Result<(), IxaError> {
    if parameters.max_time < 0.0 {
        return Err(IxaError::IxaError(
            "The max simulation running time must be non-negative.".to_string(),
        ));
    }
    // Initial conditions
    if !(0.0..=1.0).contains(&parameters.initial_incidence) {
        return Err(IxaError::IxaError(
            "The initial incidence must be between 0 and 1, inclusive.".to_string(),
        ));
    }
    // Check the infectiousness rate function
    match parameters.infectiousness_rate_fn {
        RateFnType::Constant { rate, duration } => {
            if rate < 0.0 {
                return Err(IxaError::IxaError(
                    "The infectiousness rate must be non-negative.".to_string(),
                ));
            }
            if duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The infectiousness duration must be non-negative.".to_string(),
                ));
            }
        }
    }

    // We only want to fail when all itinerary ratios are 0.
    // Instead of holding the itinerary ratios in a vector, we sum them because we error if they
    // are negative, so if their sum is 0.0, they must all be 0.0.
    // Need to ensure that keys of setting properties and itinerary ratios match.

    for setting_type in parameters.settings_properties.keys() {
        if !parameters.itinerary_ratios.contains_key(setting_type) {
            return Err(IxaError::IxaError(format!(
                "Itinerary ratios must contain all setting types defined in settings properties. Missing setting type: {:?}.",
                setting_type
            )));
        }
    }

    for setting_type in parameters.itinerary_ratios.keys() {
        if !parameters.settings_properties.contains_key(setting_type) {
            return Err(IxaError::IxaError(format!(
                "Settings properties must contain all setting types defined in itinerary ratios. Missing setting type: {:?}.",
                setting_type
            )));
        }
    }

    let mut itinerary_ratio_sum = None;

    for setting in parameters.settings_properties.values() {
        let alpha = setting.alpha;
        // Check alpha
        if !(0.0..=1.0).contains(&alpha) {
            return Err(IxaError::IxaError(
                "The alpha values for each setting must be between 0 and 1, inclusive.".to_string(),
            ));
        }
    }

    for &itinerary_ratio in parameters.itinerary_ratios.values() {
        // Check itinerary ratio
        if itinerary_ratio < 0.0 {
            return Err(IxaError::IxaError(
                "The itinerary ratio for each setting must be non-negative.".to_string(),
            ));
        }
        if let Some(sum) = itinerary_ratio_sum {
            itinerary_ratio_sum = Some(sum + itinerary_ratio);
        } else {
            itinerary_ratio_sum = Some(itinerary_ratio);
        }
    }
    if let Some(itinerary_ratio_sum) = itinerary_ratio_sum
        && itinerary_ratio_sum == 0.0
    {
        return Err(IxaError::IxaError(
            "At least one itinerary ratio must be greater than zero.".to_string(),
        ));
    }

    Ok(())
}

define_global_property!(GlobalParams, Params, validate_inputs);

pub trait ContextParametersExt: PluginContext + ContextGlobalPropertiesExt {
    fn get_params(&self) -> &Params {
        self.get_global_property_value(GlobalParams)
            .expect("Expected GlobalParams to be set")
    }
}
impl ContextParametersExt for Context {}

impl Default for Params {
    fn default() -> Self {
        Params {
            seed: 0,
            max_time: 0.0,
            synth_population_file: PathBuf::new(),
            initial_incidence: 0.0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            settings_properties: HashMap::new(),
            itinerary_ratios: HashMap::new(),
            prevalence_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
            incidence_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
            transmission_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {

    use ixa::assert_almost_eq;

    use super::*;

    #[test]
    fn test_standard_input_file() {
        let mut context = Context::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("input/input.json");
        context
            .load_global_properties(&path)
            .expect("Could not load input file");
        context.get_params();
    }

    #[test]
    fn test_default_rate_fn_type() {
        let Params {
            infectiousness_rate_fn,
            ..
        } = Params::default();

        assert_eq!(
            infectiousness_rate_fn,
            RateFnType::Constant {
                rate: 1.0,
                duration: 5.0
            }
        );
    }

    #[test]
    fn test_get_params() {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1,
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        let &Params {
            initial_incidence, ..
        } = context.get_params();
        assert_almost_eq!(initial_incidence, 0.1, 0.0);
    }

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

    #[test]
    fn test_validate_split_zeros() {
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, SettingProperties { alpha: 0.5 }),
                    (CoreSettingsTypes::School, SettingProperties { alpha: 0.5 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, 0.0),
                    (CoreSettingsTypes::School, 0.0),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "At least one itinerary ratio must be greater than zero.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that at least one itinerary ratio must be greater than zero. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_split_negative() {
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, SettingProperties { alpha: 0.5 }),
                    (CoreSettingsTypes::School, SettingProperties { alpha: 0.5 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, -0.1),
                    (CoreSettingsTypes::School, 0.0),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "The itinerary ratio for each setting must be non-negative.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that itinerary ratios cannot be negative. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_deserialization_rates() {
        let deserialized = serde_json::from_str::<RateFnType>(
            "{\"Constant\": {\"rate\": 1.0, \"duration\": 5.0}}",
        )
        .unwrap();
        assert_eq!(
            deserialized,
            RateFnType::Constant {
                rate: 1.0,
                duration: 5.0
            }
        );
    }
}
