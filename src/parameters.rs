use ixa::{HashMap, HashMapExt, HashSet, prelude::*};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, path::PathBuf};

use crate::error::ModelError;
use crate::infection_importation::ImportCasesFromFile;
use crate::reports::ReportParams;
use crate::settings::SettingCategory;
use crate::surveillance::test_manager::{Sensitivity, TestAvailability, TestType};
use crate::symptom_status_manager::{SymptomAgeGroup, SymptomDelayDistLogNormParams};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ItinerarySpecificationType {
    Constant { ratio: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RateFnType {
    /// A constant rate of infectiousness (constant hazard -> exponential waiting times) for a given
    /// duration.
    Constant { rate: f64, duration: f64 },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct TestProperties {
    pub test_type: TestType,
    pub sensitivity: Sensitivity,
    pub availability: TestAvailability,
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
    pub initial_prevalence: f64,
    /// A flag to indicate whether to import cases from a file.
    pub imported_cases_timeseries: ImportCasesFromFile,
    /// A library of infection rates to assign to infected people.
    pub infectiousness_rate_fn: RateFnType,
    /// age thresholds
    pub symptom_age_groups: Vec<SymptomAgeGroup>,
    /// Probability an infected person develops mild illness
    pub probability_mild_given_infect: f64,
    /// Parameters for log normal delay distribution from infection to mild illness
    pub infect_to_mild_delay: SymptomDelayDistLogNormParams,
    /// Probability a person with mild illness develops severe illness
    pub probability_severe_given_mild: HashMap<String, f64>,
    /// Parameters for log normal delay distribution from mild to severe illness
    pub mild_to_severe_delay: SymptomDelayDistLogNormParams,
    /// Parameters for log normal delay distribution from mild illness to resolution
    pub mild_to_resolved_delay: SymptomDelayDistLogNormParams,
    /// Probability a person with severe illness develops critical illness
    pub probability_critical_given_severe: HashMap<String, f64>,
    /// Parameters for log normal delay distribution from severe to critical illness
    pub severe_to_critical_delay: SymptomDelayDistLogNormParams,
    /// Parameters for log normal delay distribution from severe illness to resolution
    pub severe_to_resolved_delay: SymptomDelayDistLogNormParams,
    /// Probability a person with critical illness dies
    pub probability_dead_given_critical: HashMap<String, f64>,
    /// Parameters for log normal delay distribution from critical illness to death
    pub critical_to_dead_delay: SymptomDelayDistLogNormParams,
    /// Parameters for log normal delay distribution from critical illness to resolution
    pub critical_to_resolved_delay: SymptomDelayDistLogNormParams,
    /// Setting properties by setting type
    pub settings_properties: HashMap<SettingCategory, SettingProperties>,
    /// ratios used to initialize individuals itineraries by setting type.
    pub itinerary_ratios: HashMap<SettingCategory, f64>,
    // vector of test properties to define test entities.
    pub test_properties: Vec<TestProperties>,
    /// Prevalence report with a period and name required
    pub prevalence_report: ReportParams,
    /// Incidence report with a period and name required
    pub incidence_report: ReportParams,
    /// Transmission report with a name required
    pub transmission_report: ReportParams,
    /// Aggregated incident deaths report with a period and name required
    pub aggregated_deaths_report: ReportParams,
    /// Terminate run on first observed death
    pub first_death_terminates_run: bool,
}

#[allow(clippy::too_many_lines)]
fn validate_inputs(parameters: &Params) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if parameters.max_time < 0.0 {
        return Err(Box::new(ModelError::ModelError(
            "The max simulation running time must be non-negative.".to_string(),
        )));
    }
    // Initial conditions
    if !(0.0..=1.0).contains(&parameters.initial_prevalence) {
        return Err(Box::new(ModelError::ModelError(
            "The initial incidence must be between 0 and 1, inclusive.".to_string(),
        )));
    }
    // Check the infectiousness rate function
    match parameters.infectiousness_rate_fn {
        RateFnType::Constant { rate, duration } => {
            if rate < 0.0 {
                return Err(Box::new(ModelError::ModelError(
                    "The infectiousness rate must be non-negative.".to_string(),
                )));
            }
            if duration < 0.0 {
                return Err(Box::new(ModelError::ModelError(
                    "The infectiousness duration must be non-negative.".to_string(),
                )));
            }
        }
    }

    // create version of symptom_age_groups sorted by minimum age thresholds
    let mut sorted_symptom_age_groups = parameters.symptom_age_groups.clone();
    sorted_symptom_age_groups.sort();

    // check that lowest symptom age group threshold is zero
    if sorted_symptom_age_groups[0].min != 0 {
        return Err(Box::new(ModelError::ModelError(
            "lowest age threshold in symptom_age_groups must be zero".to_string(),
        )));
    }

    // check that the max threshold is >= the min threshold in each age group
    // (age is defined as a u8 and we do want to allow for a single-year group where max == min)
    for symptom_age_group in &sorted_symptom_age_groups {
        if symptom_age_group.max < symptom_age_group.min {
            return Err(Box::new(ModelError::ModelError(format!(
                "max threshold is less than min threshold for {:?}",
                symptom_age_group.label
            ))));
        }
    }

    // check that the max of each symptom age group is 1 year less than the min of the next highest symptom age group
    for i in 1..sorted_symptom_age_groups.len() {
        if sorted_symptom_age_groups[i].min - sorted_symptom_age_groups[i - 1].max != 1 {
            return Err(Box::new(ModelError::ModelError(format!(
                "difference between min threshold of {:?} and max threshold of {:?} is not 1",
                sorted_symptom_age_groups[i].label,
                sorted_symptom_age_groups[i - 1].label
            ))));
        }
    }

    // check probability_mild_given_infect
    if !(0.0..=1.0).contains(&parameters.probability_mild_given_infect) {
        return Err(Box::new(ModelError::ModelError(
            "probability_mild_given_infect is not a valid transition probability; probabilities must be between 0 and 1, inclusive.".to_string(),
        )));
    }

    // check age-specific symptom probabilities
    let symptom_probability_params = [
        (
            "probability_severe_given_mild",
            &parameters.probability_severe_given_mild,
        ),
        (
            "probability_critical_given_severe",
            &parameters.probability_critical_given_severe,
        ),
        (
            "probability_dead_given_critical",
            &parameters.probability_dead_given_critical,
        ),
    ];

    let symptom_age_group_labels: HashSet<String> = parameters
        .symptom_age_groups
        .iter()
        .map(|age_group| age_group.label.clone())
        .collect();

    for (param_name, param_value) in symptom_probability_params {
        for age_group_label in &symptom_age_group_labels {
            if !param_value.contains_key(age_group_label) {
                return Err(Box::new(ModelError::ModelError(format!(
                    "{} is missing a value for symptom age group {:?}",
                    param_name, age_group_label
                ))));
            }
        }

        for (age_group, probability) in param_value {
            if !symptom_age_group_labels.contains(age_group) {
                return Err(Box::new(ModelError::ModelError(format!(
                    "{} includes an unknown symptom age group key {:?}",
                    param_name, age_group
                ))));
            }

            if !(0.0..=1.0).contains(probability) {
                return Err(Box::new(ModelError::ModelError(format!(
                    "{} is not a valid {} for age group {:?}; probabilities must be between 0 and 1, inclusive.",
                    probability, param_name, age_group
                ))));
            }
        }
    }

    // check the symptom status delay distributions
    parameters
        .infect_to_mild_delay
        .validate("infect_to_mild_delay")?;
    parameters
        .mild_to_severe_delay
        .validate("mild_to_severe_delay")?;
    parameters
        .mild_to_resolved_delay
        .validate("mild_to_resolved_delay")?;
    parameters
        .severe_to_critical_delay
        .validate("severe_to_critical_delay")?;
    parameters
        .severe_to_resolved_delay
        .validate("severe_to_resolved_delay")?;
    parameters
        .critical_to_dead_delay
        .validate("critical_to_dead_delay")?;
    parameters
        .critical_to_resolved_delay
        .validate("critical_to_resolved_delay")?;

    // We only want to fail when all itinerary ratios are 0.
    // Instead of holding the itinerary ratios in a vector, we sum them because we error if they
    // are negative, so if their sum is 0.0, they must all be 0.0.
    // Need to ensure that keys of setting properties and itinerary ratios match.

    for setting_type in parameters.settings_properties.keys() {
        if !parameters.itinerary_ratios.contains_key(setting_type) {
            return Err(Box::new(ModelError::ModelError(format!(
                "Itinerary ratios must contain all setting types defined in settings properties. Missing setting type: {:?}.",
                setting_type
            ))));
        }
    }

    for setting_type in parameters.itinerary_ratios.keys() {
        if !parameters.settings_properties.contains_key(setting_type) {
            return Err(Box::new(ModelError::ModelError(format!(
                "Settings properties must contain all setting types defined in itinerary ratios. Missing setting type: {:?}.",
                setting_type
            ))));
        }
    }

    let mut itinerary_ratio_sum = None;

    for setting in parameters.settings_properties.values() {
        let alpha = setting.alpha;
        // Check alpha
        if !(0.0..=1.0).contains(&alpha) {
            return Err(Box::new(ModelError::ModelError(
                "The alpha values for each setting must be between 0 and 1, inclusive.".to_string(),
            )));
        }
    }

    for &itinerary_ratio in parameters.itinerary_ratios.values() {
        // Check itinerary ratio
        if itinerary_ratio < 0.0 {
            return Err(Box::new(ModelError::ModelError(
                "The itinerary ratio for each setting must be non-negative.".to_string(),
            )));
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
        return Err(Box::new(ModelError::ModelError(
            "At least one itinerary ratio must be greater than zero.".to_string(),
        )));
    }

    // Check test_properties
    let mut test_types_seen = Vec::new();
    for test_prop in &parameters.test_properties {
        // Check for duplicate test types
        if test_types_seen.contains(&test_prop.test_type) {
            return Err(Box::new(ModelError::ModelError(
                "test_properties contains duplicate test types.".to_string(),
            )));
        }
        test_types_seen.push(test_prop.test_type);

        // Check test sensitivity is between 0 and 1
        if !(0.0..=1.0).contains(&test_prop.sensitivity.0) {
            return Err(Box::new(ModelError::ModelError(
                "Test sensitivity must be between 0 and 1, inclusive.".to_string(),
            )));
        }
    }

    Ok(())
}

define_global_property!(GlobalParams, Params, validate_inputs);
define_global_property!(OrderedAgeGroupsParam, Vec<SymptomAgeGroup>);

pub trait ContextParametersExt: PluginContext + ContextGlobalPropertiesExt {
    fn get_params(&self) -> &Params {
        self.get_global_property_value(GlobalParams)
            .expect("Expected GlobalParams to be set")
    }
}
impl ContextParametersExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params {
        symptom_age_groups, ..
    } = context.get_params();

    let mut ordered_symptom_age_groups = symptom_age_groups.clone();
    ordered_symptom_age_groups.sort();
    let _ = context.set_global_property_value(OrderedAgeGroupsParam, ordered_symptom_age_groups);
    Ok(())
}

impl Default for Params {
    fn default() -> Self {
        Params {
            seed: 0,
            max_time: 0.0,
            synth_population_file: PathBuf::new(),
            initial_prevalence: 0.0,
            imported_cases_timeseries: ImportCasesFromFile {
                include: false,
                filename: None,
            },
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_age_groups: vec![SymptomAgeGroup {
                label: "Age0To120".to_string(),
                min: 0,
                max: 120,
            }],
            probability_mild_given_infect: 0.0,
            infect_to_mild_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            probability_severe_given_mild: HashMap::from_iter([("Age0To120".to_string(), 0.0)]),
            mild_to_severe_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            mild_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            probability_critical_given_severe: HashMap::from_iter([("Age0To120".to_string(), 0.0)]),
            severe_to_critical_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            severe_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            probability_dead_given_critical: HashMap::from_iter([("Age0To120".to_string(), 0.0)]),
            critical_to_dead_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            critical_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            settings_properties: HashMap::new(),
            itinerary_ratios: HashMap::new(),
            test_properties: Vec::new(),
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
            aggregated_deaths_report: ReportParams {
                write: false,
                filename: None,
                period: None,
            },
            first_death_terminates_run: false,
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
            initial_prevalence: 0.1,
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        let &Params {
            initial_prevalence, ..
        } = context.get_params();
        assert_almost_eq!(initial_prevalence, 0.1, 0.0);
    }

    #[test]
    fn test_validate_max_time() {
        let parameters = Params {
            max_time: -100.0,
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(err) => match *err.downcast::<ModelError>().unwrap() {
                ModelError::ModelError(msg) => {
                    assert_eq!(
                        msg,
                        "The max simulation running time must be non-negative.".to_string()
                    );
                }
                ue => panic!(
                    "Expected an error that the max simulation running time validation should fail. Instead got {:?}",
                    ue.to_string()
                ),
            },
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_split_zeros() {
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.5 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.5 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter(
                [(SettingCategory::Home, 0.0), (SettingCategory::School, 0.0)]
                    .into_iter()
                    .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(err) => match *err.downcast::<ModelError>().unwrap() {
                ModelError::ModelError(msg) => {
                    assert_eq!(
                        msg,
                        "At least one itinerary ratio must be greater than zero.".to_string()
                    );
                }
                ue => panic!(
                    "Expected an error that at least one itinerary ratio must be greater than zero. Instead got {:?}",
                    ue.to_string()
                ),
            },
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_split_negative() {
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.5 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.5 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter(
                [
                    (SettingCategory::Home, -0.1),
                    (SettingCategory::School, 0.0),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(err) => match *err.downcast::<ModelError>().unwrap() {
                ModelError::ModelError(msg) => {
                    assert_eq!(
                        msg,
                        "The itinerary ratio for each setting must be non-negative.".to_string()
                    );
                }
                ue => panic!(
                    "Expected an error that itinerary ratios cannot be negative. Instead got {:?}",
                    ue.to_string()
                ),
            },
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

    #[test]
    fn test_sort_age_groups_in_init() {
        let age_groups = vec![
            SymptomAgeGroup {
                label: "Age18To49".to_string(),
                min: 18,
                max: 49,
            },
            SymptomAgeGroup {
                label: "Age0To17".to_string(),
                min: 0,
                max: 17,
            },
            SymptomAgeGroup {
                label: "Age50To64".to_string(),
                min: 50,
                max: 64,
            },
            SymptomAgeGroup {
                label: "Age65Plus".to_string(),
                min: 65,
                max: 120,
            },
        ];
        let mut context = Context::new();
        let parameters = Params {
            symptom_age_groups: age_groups.clone(),
            probability_severe_given_mild: ixa::HashMap::from_iter([
                ("Age0To17".to_string(), 0.0),
                ("Age18To49".to_string(), 1.0),
                ("Age50To64".to_string(), 1.0),
                ("Age65Plus".to_string(), 1.0),
            ]),
            probability_critical_given_severe: ixa::HashMap::from_iter([
                ("Age0To17".to_string(), 0.0),
                ("Age18To49".to_string(), 0.0),
                ("Age50To64".to_string(), 1.0),
                ("Age65Plus".to_string(), 1.0),
            ]),
            probability_dead_given_critical: ixa::HashMap::from_iter([
                ("Age0To17".to_string(), 0.0),
                ("Age18To49".to_string(), 0.0),
                ("Age50To64".to_string(), 0.0),
                ("Age65Plus".to_string(), 1.0),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        init(&mut context).unwrap();
        let ordered_age_groups = context
            .get_global_property_value(OrderedAgeGroupsParam)
            .unwrap();
        assert_eq!(
            *ordered_age_groups,
            vec![
                SymptomAgeGroup {
                    label: "Age0To17".to_string(),
                    min: 0,
                    max: 17,
                },
                SymptomAgeGroup {
                    label: "Age18To49".to_string(),
                    min: 18,
                    max: 49,
                },
                SymptomAgeGroup {
                    label: "Age50To64".to_string(),
                    min: 50,
                    max: 64,
                },
                SymptomAgeGroup {
                    label: "Age65Plus".to_string(),
                    min: 65,
                    max: 120,
                },
            ]
        );
    }
}
