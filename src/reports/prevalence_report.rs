use crate::{
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    population_loader::{Age, Alive, Person},
    symptom_status_manager::SymptomStatus,
};
use ixa::prelude::*;
use ixa::{ExecutionPhase, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyReport {
    t: f64,
    age: u8,
    infection_status: InfectionStatus,
    symptom_status: SymptomStatus,
    count: usize,
}

define_report!(PersonPropertyReport);

define_multi_property!(Person, (Age, InfectionStatus, SymptomStatus));

struct PropertyReportDataContainer {
    report_map_container: HashMap<(Age, InfectionStatus, SymptomStatus), usize>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        report_map_container: HashMap::default()
    }
);

type ReportEvent = PropertyChangeEvent<Person, (Age, InfectionStatus, SymptomStatus)>;

fn update_change_counts(context: &mut Context, event: ReportEvent) {
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
            symptom_status: values.2,
            count: *count_property,
        });
    }
}

/// Count initial number of people per property status and subscribe to changes
/// # Errors
///
/// Will return `ModelError` if the report cannot be added
///
/// # Panics
///
/// Will panic if symptom value string is not listed in enum
pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), ModelError> {
    context.add_report::<PersonPropertyReport>(file_name)?;

    let mut map_counts = HashMap::default();

    context.with_query_results(with!(Person, Alive(true)), &mut |current_people| {
        //current_people = results.to_owned_vec();
        for person in current_people {
            let value: (Age, InfectionStatus, SymptomStatus) = context.get_property(person);
            map_counts
                .entry(value)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
    });

    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container.report_map_container = map_counts;

    context.subscribe_to_event::<ReportEvent>(|context, event| {
        update_change_counts(context, event);
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
    use crate::population_loader::Person;
    use crate::{
        Age,
        infectiousness_manager::{InfectionContextExt, InfectionStatus},
        parameters::{ContextParametersExt, GlobalParams, Params},
        population_loader::PersonId,
        rate_fns::load_rate_fns,
        reports::ReportParams,
        settings::SettingCode,
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

        let source: PersonId = context.add_entity(with!(Person, Age(42))).unwrap();
        let target: PersonId = context.add_entity(with!(Person, Age(43))).unwrap();
        let home: SettingCode = SettingCode::arbitrary_home_code();
        let setting = Some(home);
        let infection_time = 1.0;

        context.infect_person(source, None, None);
        crate::reports::init(&mut context).unwrap();

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting);
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
        let mut line_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::prevalence_report::PersonPropertyReport = result.unwrap();
            line_count += 1;
            if record.t == 0.0 {
                if record.age == 42 {
                    assert_eq!(record.infection_status, InfectionStatus::Infectious);
                    assert_eq!(record.count, 1);
                } else if record.age == 43 {
                    assert_eq!(record.infection_status, InfectionStatus::Susceptible);
                    assert_eq!(record.count, 1);
                } else {
                    panic!("invalid age at t == 0.0")
                }
            } else if record.t == 2.0 {
                if record.age == 42 {
                    assert_eq!(record.infection_status, InfectionStatus::Infectious);
                    assert_eq!(record.count, 1);
                } else if record.age == 43 {
                    match record.infection_status {
                        InfectionStatus::Susceptible => assert_eq!(record.count, 0),
                        InfectionStatus::Infectious => assert_eq!(record.count, 1),
                        _ => panic!("All InfectionStatus should be susceptible or infectious"),
                    }
                } else {
                    panic!("invalid age at t == 2.0")
                }
            } else {
                panic!("record times other than 0.0 and 2.0 are invalid")
            }
        }

        assert_eq!(line_count, 5);
    }
}
