use crate::{
    error::ModelError,
    parameters::{ContextParametersExt, Params},
};
use ixa::prelude::*;
use serde::{Deserialize, Serialize};

pub mod incidence_report;
pub mod prevalence_report;
pub mod transmission_report;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReportParams {
    pub write: bool,
    pub filename: Option<String>,
    pub period: Option<f64>,
}

fn get_report_name(params: &ReportParams) -> Result<Option<&str>, ModelError> {
    if params.write {
        if let Some(name) = &params.filename {
            return Ok(Some(name));
        }

        return Err(ModelError::ModelError(
            "Reports must be provided with a name when write is set to true".to_string(),
        ));
    }

    if let Some(name) = &params.filename {
        info!("Report {name} is off but has associated values with name.");
    }
    Ok(None)
}

fn get_period_report_name(params: &ReportParams) -> Result<Option<(&str, f64)>, ModelError> {
    if let Some(name) = get_report_name(params)? {
        if let Some(period) = params.period {
            if period <= 0.0 {
                return Err(ModelError::ModelError(format!(
                    "The report period must be greater than zero, found period {period} for {name} instead."
                )));
            }
            return Ok(Some((name, period)));
        }

        return Err(ModelError::ModelError(format!(
            "Report {name} requires a period but none provided."
        )));
    }
    Ok(None)
}

/// # Errors
///
/// Will return `ModelError` if any report within the reports list cannot be added
/// or if the period for any periodic report is less than 0.0
pub fn init(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        prevalence_report,
        incidence_report,
        transmission_report,
        ..
    } = context.get_params().clone();
    let mut report_count = 0;

    if let Some((name, period)) = get_period_report_name(&prevalence_report)? {
        prevalence_report::init(context, name, period)?;
        info!("Generating the prevalence report.");
        report_count += 1;
    }
    if let Some((name, period)) = get_period_report_name(&incidence_report)? {
        incidence_report::init(context, name, period)?;
        info!("Generating the incidence report.");
        report_count += 1;
    }
    if let Some(name) = get_report_name(&transmission_report)? {
        transmission_report::init(context, name)?;
        info!("Generating the transmission report.");
        report_count += 1;
    }

    info!("Generating {report_count} report(s) in total.");

    Ok(())
}

#[cfg(test)]
mod test {

    use super::get_period_report_name;
    use crate::error::ModelError;
    use crate::reports::ReportParams;
    use crate::{
        parameters::{ContextParametersExt, Params},
        rate_fns::load_rate_fns,
    };
    use ixa::assert_almost_eq;
    use ixa::{Context, ContextGlobalPropertiesExt, ContextRandomExt};
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_from_str(params_json: &str) -> Context {
        let temp_dir = tempdir().unwrap();
        let dir = PathBuf::from(&temp_dir.path());
        let file_path = dir.join("input.json");
        let mut file = File::create(file_path.clone()).unwrap();
        file.write_all(params_json.as_bytes()).unwrap();

        let mut context = Context::new();
        context.load_global_properties(&file_path).unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_list_reports() {
        let params_json = r#"
            {
                "epimodel.GlobalParams": {
                    "seed": 123,
                    "max_time": 200.0,
                    "synth_population_file": "input/people_test.csv",
                    "initial_prevalence": 0.01,
                    "imported_cases_timeseries": {
                        "include": false
                    },
                    "infectiousness_rate_fn": {"Constant": {"rate": 1.0, "duration": 5.0}},
                    "symptom_age_groups": [{"label": "Age0To17", "min": 0, "max": 17},
                        {"label": "Age18To49", "min": 18, "max": 49},
                        {"label": "Age50To64", "min": 50, "max": 64},
                        {"label": "Age65Plus", "min": 65, "max": 120}],
                    "probability_mild_given_infect": 0.7,
                    "infect_to_mild_delay": {"mu": 0.1, "sigma": 0.0},
                    "probability_severe_given_mild": {"Age0To17": 0.004, "Age18To49": 0.034, "Age50To64": 0.108, "Age65Plus": 0.684},
                    "mild_to_severe_delay": {"mu": 0.1, "sigma": 0.1},
                    "mild_to_resolved_delay": {"mu": 0.1, "sigma": 0.1},
                    "probability_critical_given_severe": {"Age0To17": 0.275, "Age18To49": 0.189, "Age50To64": 0.271, "Age65Plus": 0.269},
                    "severe_to_critical_delay": {"mu": 0.1, "sigma": 0.1},
                    "severe_to_resolved_delay": {"mu": 0.1, "sigma": 0.1},
                    "probability_dead_given_critical": {"Age0To17": 0.026, "Age18To49": 0.111, "Age50To64": 0.292, "Age65Plus": 0.699},
                    "critical_to_dead_delay": {"mu": 0.1, "sigma": 0.1},
                    "critical_to_resolved_delay": {"mu": 0.1, "sigma": 0.1},
                    "settings_properties": {},
                    "itinerary_ratios": {},
                    "prevalence_report": {
                        "write": true,
                        "filename": "prevalence.csv",
                        "period": 1.0
                    },
                    "incidence_report": {
                        "write": true,
                        "filename": "incidence.csv",
                        "period": 2.0
                    },
                    "transmission_report": {
                        "write": true,
                        "filename": "transmission.csv"
                    }
                }
            }
        "#;
        let context = setup_context_from_str(params_json);
        let Params {
            prevalence_report,
            incidence_report,
            transmission_report,
            ..
        } = context.get_params().clone();

        assert!(prevalence_report.write);
        assert_eq!(
            prevalence_report.filename,
            Some("prevalence.csv".to_string())
        );
        assert_eq!(prevalence_report.period, Some(1.0));

        assert!(incidence_report.write);
        assert_eq!(incidence_report.filename, Some("incidence.csv".to_string()));
        assert_eq!(incidence_report.period, Some(2.0));

        assert!(transmission_report.write);
        assert_eq!(
            transmission_report.filename,
            Some("transmission.csv".to_string())
        );
        assert_eq!(transmission_report.period, None);
    }

    #[test]
    fn test_get_period_report_name() {
        let name = "output.csv".to_string();
        let period = 3.0;

        let report = ReportParams {
            write: true,
            filename: Some(name.clone()),
            period: Some(period),
        };

        if let Some((expect_name, expect_period)) = get_period_report_name(&report).unwrap() {
            assert_eq!(name, *expect_name);
            assert_almost_eq!(period, expect_period, 0.0);
        } else {
            panic!("Expected some value for the validated name and period");
        }
    }

    #[test]
    fn test_get_period_report_name_nowrite() {
        let name = "output.csv".to_string();
        let period = 3.0;

        let report = ReportParams {
            write: false,
            filename: Some(name),
            period: Some(period),
        };

        assert_eq!(None, get_period_report_name(&report).unwrap());
    }

    #[test]
    fn test_error_no_name() {
        let period = 3.0;

        let no_name_report = ReportParams {
            write: true,
            filename: None,
            period: Some(period),
        };

        match get_period_report_name(&no_name_report).err() {
            Some(ModelError::ModelError(msg)) => {
                assert_eq!(
                    msg,
                    "Reports must be provided with a name when write is set to true".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error the report name is required. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead validation passed with no errors."),
        }
    }

    #[test]
    fn test_error_bad_period() {
        let name = "output.csv".to_string();
        let bad_period = 0.0;

        let bad_period_report = ReportParams {
            write: true,
            filename: Some(name),
            period: Some(bad_period),
        };

        match get_period_report_name(&bad_period_report).err() {
            Some(ModelError::ModelError(msg)) => {
                assert_eq!(
                    msg,
                    "The report period must be greater than zero, found period 0 for output.csv instead.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error the report name is required. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead validation passed with no errors."),
        }
    }
}
