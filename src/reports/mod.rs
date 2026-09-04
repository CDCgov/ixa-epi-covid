use crate::{
    error::ModelError,
    parameters::{ContextParametersExt, Params},
};
use ixa::prelude::*;
use serde::{Deserialize, Serialize};

pub mod aggregated_deaths_report;
pub mod incidence_report;
pub mod prevalence_report;
pub mod transmission_report;
pub mod current_hospitalizations_report;
pub mod attack_rate_report;

// the skip_serializing_if is used to avoid having the period field show up in the json in the tests
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReportParams {
    pub write: bool,
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        aggregated_deaths_report,
        current_hospitalizations_report,
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
    if let Some((name, period)) = get_period_report_name(&aggregated_deaths_report)? {
        aggregated_deaths_report::init(context, name, period)?;
        info!("Generating the aggregated deaths incidence report.");
        report_count += 1;
    }

    if let Some((name, period)) = get_period_report_name(&current_hospitalizations_report)?{
        current_hospitalizations_report::init(context, name, period)?;
        info!("Generating the current hospitalizations report.");
        report_count += 1;
    }

    info!("Generating {report_count} report(s) in total.");

    Ok(())
}

#[cfg(test)]
mod test {

    use super::get_period_report_name;
    use crate::error::ModelError;
    use crate::parameters::GlobalParams;
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

    fn setup_context_for_reports(
        incidence_report: ReportParams,
        prevalence_report: ReportParams,
        transmission_report: ReportParams,
        aggregated_deaths_report: ReportParams,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            incidence_report,
            prevalence_report,
            transmission_report,
            aggregated_deaths_report,
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        context
    }

    #[test]
    fn test_list_reports() {
        let prevalence_report = ReportParams {
            write: true,
            filename: Some("prevalence.csv".to_string()),
            period: Some(1.0),
        };
        let incidence_report = ReportParams {
            write: true,
            filename: Some("incidence.csv".to_string()),
            period: Some(2.0),
        };
        let transmission_report = ReportParams {
            write: true,
            filename: Some("transmission.csv".to_string()),
            period: None,
        };
        let aggregated_deaths_report = ReportParams {
            write: true,
            filename: Some("aggregated_deaths.csv".to_string()),
            period: Some(3.0),
        };

        let context = setup_context_for_reports(
            incidence_report,
            prevalence_report,
            transmission_report,
            aggregated_deaths_report,
        );
        let params = context.get_params().clone();

        let mut wrapped = serde_json::json!({});
        wrapped["epimodel.GlobalParams"] = serde_json::to_value(&params).unwrap();
        let params_str = serde_json::to_string_pretty(&wrapped).unwrap();

        let context = setup_context_from_str(&params_str);
        let Params {
            prevalence_report,
            incidence_report,
            transmission_report,
            aggregated_deaths_report,
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

        assert!(aggregated_deaths_report.write);
        assert_eq!(
            aggregated_deaths_report.filename,
            Some("aggregated_deaths.csv".to_string())
        );
        assert_eq!(aggregated_deaths_report.period, Some(3.0));
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
