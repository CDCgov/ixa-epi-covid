use crate::{
    clinical_status::SymptomStatus,
    infectiousness_manager::InfectionStatus,
    population_loader::{Age, Alive, Person},
};
use ixa::prelude::*;
use ixa::{ExecutionPhase, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyReport {
    t: f64,
    age: u8,
    infection_status: InfectionStatus,
    symptoms: SymptomStatus,
    count: usize,
}

define_report!(PersonPropertyReport);

define_multi_property!((Age, InfectionStatus, SymptomStatus), Person);

struct PropertyReportDataContainer {
    report_map_container: HashMap<(Age, InfectionStatus, SymptomStatus), usize>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        report_map_container: HashMap::default(),
    }
);

type ReportEvent = PropertyChangeEvent<Person, (Age, InfectionStatus, SymptomStatus)>;

fn update_property_change_counts(context: &mut Context, event: ReportEvent) {
    let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);

    let _ = *report_container_mut
        .report_map_container
        .entry(event.current)
        .and_modify(|n| *n += 1)
        .or_insert(1);

    let _ = *report_container_mut
        .report_map_container
        .entry(event.previous)
        .and_modify(|n| *n -= 1)
        .or_insert(0);
}

fn send_property_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);

    for (values, count_property) in &report_container.report_map_container {
        context.send_report(PersonPropertyReport {
            t: context.get_current_time(),
            age: values.0.0,
            infection_status: values.1,
            symptoms: values.2,
            count: *count_property,
        });
    }
}

/// Count initial number of people per property status and subscribe to cahnges
/// # Errors
///
/// Will return `IxaError` if the report cannot be added
///
/// # Panics
///
/// Will panic if symptom value string is not listed in enum
pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), IxaError> {
    context.add_report::<PersonPropertyReport>(file_name)?;

    let mut map_counts = HashMap::default();
    context.with_query_results::<Person, _>((Alive(true),), &mut |current_people| {
        //current_people = results.to_owned_vec();
        for person in current_people {
            let value: (Age, InfectionStatus, SymptomStatus) = context.get_property(*person);
            map_counts
                .entry(value)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
    });

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container.report_map_container = map_counts;

    context.subscribe_to_event::<ReportEvent>(|context, event| {
        update_property_change_counts(context, event);
    });

    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_property_counts(context);
        },
        ExecutionPhase::Last,
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        Age,
        infectiousness_manager::InfectionContextExt,
        parameters::{ContextParametersExt, GlobalParams, Params},
        population_loader::PersonId,
        rate_fns::load_rate_fns,
        reports::ReportParams,
    };
    use ixa::{csv, prelude::*};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(prevalence_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    prevalence_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_generate_prevalence_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: Some(2.0),
        });

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let source: PersonId = context.add_entity((Age(42),)).unwrap();
        let target: PersonId = context.add_entity((Age(43),)).unwrap();
        let setting_type = Some("test_setting");
        let setting_id: Option<usize> = Some(1);
        let infection_time = 1.0;

        context.infect_person(source, None, None, None);
        crate::reports::init(&mut context).unwrap();

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting_type, setting_id);
        });
        context.execute();

        let Params {
            prevalence_report, ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = prevalence_report.filename {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();

        let mut actual: Vec<Vec<String>> = reader
            .records()
            .map(|result| result.unwrap().iter().map(String::from).collect())
            .collect();
        let mut expected = vec![
            //   t    | age | inf status | count
            vec!["0.0", "42", "Infectious", "1"],
            vec!["0.0", "43", "Susceptible", "1"],
            vec!["2.0", "42", "Infectious", "1"],
            vec!["2.0", "43", "Infectious", "1"],
            // Only an initialized combination can have a zero count
            vec!["2.0", "43", "Susceptible", "0"],
        ];

        actual.sort();
        expected.sort();

        assert_eq!(actual, expected, "CSV file should contain the correct data");
    }
}
