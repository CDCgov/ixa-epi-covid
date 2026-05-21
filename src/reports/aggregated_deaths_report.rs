use crate::{error::ModelError, population_loader::Person, symptom_status_manager::SymptomStatus};
use ixa::{ExecutionPhase, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct AggregatedDeathsIncidenceReport {
    t_upper: f64,
    count: u32,
}

define_report!(AggregatedDeathsIncidenceReport);

struct PropertyReportDataContainer {
    death_status_change: u32,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        death_status_change: 0
    }
);

fn update_death_incidence(
    context: &mut Context,
    event: PropertyChangeEvent<Person, SymptomStatus>,
) {
    if event.current == SymptomStatus::Dead {
        let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
        report_container_mut.death_status_change += 1;
    }
}

fn reset_incidence_map(context: &mut Context) {
    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container.death_status_change = 0;
}

fn send_incidence_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);
    let t_upper = context.get_current_time();
    context.send_report(AggregatedDeathsIncidenceReport {
        t_upper,
        count: report_container.death_status_change,
    });
    reset_incidence_map(context);
}

/// # Errors
///
/// Will return `ModelError` if the report cannot be added
///
/// # Panics
///
/// Will panic if an age group cannot be parsed from the tabulated string
pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), ModelError> {
    context.add_report::<AggregatedDeathsIncidenceReport>(file_name)?;

    context.subscribe_to_event::<PropertyChangeEvent<Person, SymptomStatus>>(|context, event| {
        update_death_incidence(context, event);
    });

    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_incidence_counts(context);
        },
        ExecutionPhase::Last,
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        parameters::{ContextParametersExt, GlobalParams, Params},
        population_loader::{Age, Person, PersonId},
        rate_fns::load_rate_fns,
        reports::ReportParams,
        symptom_status_manager::SymptomData,
    };
    use ixa::{csv, prelude::*};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(aggregated_deaths_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    aggregated_deaths_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_generate_incidence_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: Some(2.0),
        });

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());
        crate::reports::init(&mut context).unwrap();

        let _survivor: PersonId = context.add_entity(with!(Person, Age(42))).unwrap();
        let target: PersonId = context.add_entity(with!(Person, Age(43))).unwrap();
        let time_of_death = 1.0;

        context.add_plan(time_of_death, move |context| {
            context.set_property::<Person, SymptomData>(
                target,
                SymptomData::Dead {
                    mild_time: 0.0,
                    severe_time: 0.0,
                    critical_time: 0.0,
                    dead_time: time_of_death,
                },
            );
        });
        context.execute();

        let Params {
            aggregated_deaths_report,
            ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = aggregated_deaths_report.filename {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut event_count = 0;
        let mut line_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::aggregated_deaths_report::AggregatedDeathsIncidenceReport =
                result.unwrap();
            line_count += 1;
            if record.t_upper == 2.0 {
                assert_eq!(record.count, 1);
                event_count += 1;
            } else {
                assert_eq!(record.count, 0);
            }
        }

        assert!(line_count > event_count);
        assert_eq!(event_count, 1);
    }
}
