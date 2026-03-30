use crate::{
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    population_loader::{Age, Person},
    symptom_status_manager::SymptomStatus,
};
use ixa::{ExecutionPhase, HashMap, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyIncidenceReport {
    t_upper: f64,
    age: u8,
    event: String,
    count: u32,
}

define_report!(PersonPropertyIncidenceReport);

struct PropertyReportDataContainer {
    infection_status_change: HashMap<(u8, InfectionStatus), u32>,
    symptom_status_change: HashMap<(u8, SymptomStatus), u32>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        infection_status_change: HashMap::default(),
        symptom_status_change: HashMap::default()
    }
);

fn update_infection_incidence(
    context: &mut Context,
    event: PropertyChangeEvent<Person, InfectionStatus>,
) {
    if event.current == InfectionStatus::Infectious || event.current == InfectionStatus::Recovered {
        let age: Age = context.get_property(event.entity_id);
        let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
        report_container_mut
            .infection_status_change
            .entry((age.0, event.current))
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }
}

fn update_symptom_incidence(
    context: &mut Context,
    event: PropertyChangeEvent<Person, SymptomStatus>,
) {
    if event.current != SymptomStatus::NoSymptoms {
        let age: Age = context.get_property(event.entity_id);
        let report_container_mut = context.get_data_mut(PropertyReportDataPlugin);
        report_container_mut
            .symptom_status_change
            .entry((age.0, event.current))
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }
}

fn reset_incidence_map(context: &mut Context) {
    let report_container = context.get_data_mut(PropertyReportDataPlugin);
    report_container
        .infection_status_change
        .values_mut()
        .for_each(|v| *v = 0);
    report_container
        .symptom_status_change
        .values_mut()
        .for_each(|v| *v = 0);
}

fn send_incidence_counts(context: &mut Context) {
    let report_container = context.get_data(PropertyReportDataPlugin);
    let t_upper = context.get_current_time();

    // Infection status
    for ((age, infection_status), count) in &report_container.infection_status_change {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{infection_status:?}"),
            count: *count,
        });
    }
    // Symptom status
    for ((age, symptom_status), count) in &report_container.symptom_status_change {
        context.send_report(PersonPropertyIncidenceReport {
            t_upper,
            age: *age,
            event: format!("{symptom_status:?}"),
            count: *count,
        });
    }
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
    context.add_report::<PersonPropertyIncidenceReport>(file_name)?;

    // let tabulator = (Age,);
    // let ages: RefCell<HashSet<u8>> = RefCell::new(HashSet::new());
    // context.tabulate_person_properties(&tabulator, |_context, values, _count| {
    //     ages.borrow_mut().insert(values[0].parse::<u8>().unwrap());
    // });

    let mut ages: Vec<u8> = Vec::new();
    for person in context.get_entity_iterator::<Person>() {
        let age: Age = context.get_property(person);
        if !ages.contains(&age.0) {
            ages.push(age.0);
        }
    }

    let report_container = context.get_data_mut(PropertyReportDataPlugin);

    for age in ages {
        let inf_vec = [InfectionStatus::Infectious, InfectionStatus::Recovered];

        for inf_value in inf_vec {
            report_container
                .infection_status_change
                .insert((age, inf_value), 0);
        }

        let symp_vec = [
            SymptomStatus::Mild,
            SymptomStatus::Severe,
            SymptomStatus::Critical,
            SymptomStatus::Dead,
            SymptomStatus::Resolved,
        ];

        for symp_value in symp_vec {
            report_container
                .symptom_status_change
                .insert((age, symp_value), 0);
        }
    }

    context.subscribe_to_event::<PropertyChangeEvent<Person, InfectionStatus>>(|context, event| {
        update_infection_incidence(context, event);
    });

    context.subscribe_to_event::<PropertyChangeEvent<Person, SymptomStatus>>(|context, event| {
        update_symptom_incidence(context, event);
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
        infectiousness_manager::InfectionContextExt,
        parameters::{ContextParametersExt, GlobalParams, Params},
        population_loader::{Age, Person, PersonId},
        rate_fns::load_rate_fns,
        reports::ReportParams,
        settings::{Alpha, Setting, SettingCategory, SettingCode},
    };
    use ixa::{csv, prelude::*};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(incidence_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    incidence_report,
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

        let source: PersonId = context.add_entity((Age(42),)).unwrap();
        let target: PersonId = context.add_entity((Age(43),)).unwrap();
        let home = context
            .add_entity::<Setting, _>((SettingCategory::Home, SettingCode(0), Alpha(0.0)))
            .unwrap();
        let setting = Some(home);
        let infection_time = 1.0;

        context.infect_person(source, None, None);
        crate::reports::init(&mut context).unwrap();

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting);
        });
        context.execute();

        let Params {
            incidence_report, ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = incidence_report.filename {
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
            let record: crate::reports::incidence_report::PersonPropertyIncidenceReport =
                result.unwrap();
            line_count += 1;
            if record.t_upper == 2.0 && record.event == *"Infectious" && record.age == 43 {
                assert_eq!(record.count, 1);
                event_count += 1;
            } else {
                assert_eq!(record.count, 0);
            }
        }

        assert!(line_count > event_count);
        assert_eq!(event_count, 1);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_age_change() {
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
        let home = context
            .add_entity::<Setting, _>((SettingCategory::Home, SettingCode(0), Alpha(0.0)))
            .unwrap();
        let setting = Some(home);
        let infection_time = 1.0;

        context.infect_person(source, None, None);
        crate::reports::init(&mut context).unwrap();

        context.add_plan(infection_time, move |context| {
            context.infect_person(target, Some(source), setting);
        });
        context.add_plan(infection_time - 0.1, move |context| {
            context.set_property::<Person, Age>(target, Age(44));
        });
        context.execute();

        let Params {
            incidence_report, ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = incidence_report.filename {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut line_count = 0;
        let mut event_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::incidence_report::PersonPropertyIncidenceReport =
                result.unwrap();
            line_count += 1;
            if record.t_upper == 2.0 && record.event == *"Infectious" && record.age == 44 {
                assert_eq!(record.count, 1);
                event_count += 1;
            } else {
                assert_eq!(record.count, 0);
            }
        }

        // 2 event types: Infectious + Recovered
        // 2 time points
        // 2 ages at first timepoint, 3 ages at second timepoint for only one event (2x2x2 + 1 = 9)
        assert!(line_count > event_count);
        assert_eq!(event_count, 1);
    }
}
