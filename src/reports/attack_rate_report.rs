use crate::{
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    parameters::AttackRateAgeGroupsParam,
    population_loader::{Age, Person},
};
use ixa::{ExecutionPhase, prelude::*};
use serde::{Deserialize, Serialize};

define_derived_property!(
    struct AttackRateAgeGroupIndex(usize),
    Person,
    [Age],
    [AttackRateAgeGroupsParam],
    |age, ordered_age_groups| {
        let mut found_age_group = false;
        let mut index = 0;
        while index < ordered_age_groups.len() {
            if age.0 >= ordered_age_groups[index].min && age.0 <= ordered_age_groups[index].max {
                found_age_group = true;
                break;
            }
            index += 1;
        }
        if !found_age_group {
            panic!("No valid age group for age");
        }
        return AttackRateAgeGroupIndex(index);
    }
);

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct AttackRateReport {
    t_upper: f64,
    age_group: String,
    attack_rate: f64,
}

define_report!(AttackRateReport);

struct AttackRateReportDataContainer {
    age_group_attack_rate: Vec<usize>,
    age_group_population: Vec<usize>,
}

define_data_plugin!(
    AttackRateReportDataPlugin,
    AttackRateReportDataContainer,
    |context| {
        let num_age_groups = context
            .get_global_property_value(AttackRateAgeGroupsParam)
            .expect("Expected GlobalParams to be set")
            .len();
        AttackRateReportDataContainer {
            age_group_attack_rate: vec![0; num_age_groups],
            age_group_population: vec![0; num_age_groups],
        }
    }
);

type ReportEvent = PropertyChangeEvent<Person, InfectionStatus>;

fn update_change_counts(context: &mut Context, event: ReportEvent) {
    let age_group_index = context.get_property::<Person, AttackRateAgeGroupIndex>(event.entity_id);
    let report_container_mut = context.get_data_mut(AttackRateReportDataPlugin);
    if event.current == InfectionStatus::Infectious {
        report_container_mut.age_group_attack_rate[age_group_index.0] += 1;
    }
}

fn set_age_group_populations(context: &mut Context) {
    let mut age_group_population = context
        .get_data(AttackRateReportDataPlugin)
        .age_group_population
        .clone();
    if let Some(age_groups) = context.get_global_property_value(AttackRateAgeGroupsParam) {
        for (index, _age_group) in age_groups.iter().enumerate() {
            age_group_population[index] =
                context.query_entity_count(with!(Person, AttackRateAgeGroupIndex(index)));
        }
    }
}

fn send_property_counts(context: &mut Context) {
    let report_container = context.get_data(AttackRateReportDataPlugin);
    for (index, age_group) in context
        .get_global_property_value(AttackRateAgeGroupsParam)
        .expect("Expected GlobalParams to be set")
        .iter()
        .enumerate()
    {
        context.send_report(AttackRateReport {
            t_upper: context.get_current_time(),
            age_group: age_group.label.clone(),
            attack_rate: report_container.age_group_attack_rate[index] as f64
                / report_container.age_group_population[index] as f64,
        });
    }
}

pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), ModelError> {
    context.add_report::<AttackRateReport>(file_name)?;

    set_age_group_populations(context);

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
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::AgeGroup,
        parameters::{AttackRateAgeGroupsParam, ContextParametersExt, GlobalParams, Params},
        population_loader::{Age, Person, PersonId},
        rate_fns::load_rate_fns,
        reports::ReportParams,
    };
    use ixa::{assert_almost_eq, csv, prelude::*};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(attack_rate_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    attack_rate_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context
            .set_global_property_value(
                AttackRateAgeGroupsParam,
                vec![
                    AgeGroup {
                        label: "Age0To49".to_string(),
                        min: 0,
                        max: 49,
                    },
                    AgeGroup {
                        label: "Age50To120".to_string(),
                        min: 50,
                        max: 120,
                    },
                ],
            )
            .unwrap();

        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_generate_attack_rate_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: Some(2.0),
        });

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let p1: PersonId = context.add_entity(with!(Person, Age(40))).unwrap();
        let p2: PersonId = context.add_entity(with!(Person, Age(43))).unwrap();
        let _p3: PersonId = context.add_entity(with!(Person, Age(42))).unwrap();
        let p4: PersonId = context.add_entity(with!(Person, Age(60))).unwrap();
        let p5: PersonId = context.add_entity(with!(Person, Age(65))).unwrap();
        let p6: PersonId = context.add_entity(with!(Person, Age(70))).unwrap();

        crate::reports::init(&mut context).unwrap();

        context.add_plan(1.0, move |context| {
            context.infect_person(p1, None, None);
            context.infect_person(p2, None, None);
        });

        context.add_plan(2.0, move |context| {
            context.infect_person(p4, None, None);
        });

        context.add_plan(3.0, move |context| {
            context.infect_person(p5, None, None);
            context.infect_person(p6, None, None);
        });

        context.add_plan(6.0, move |context| {
            context.shutdown();
        });
        context.execute();

        let Params {
            attack_rate_report, ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = attack_rate_report.filename {
            path.join(name)
        } else {
            panic!("No report name specified");
        };
        println!("File path: {:?}", file_path);
        assert!(file_path.exists());
        std::mem::drop(context);

        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut line_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::attack_rate_report::AttackRateReport = result.unwrap();
            line_count += 1;
            if record.t_upper == 2.0 && record.age_group == *"Age0to49" {
                assert_almost_eq!(record.attack_rate, 2.0 / 3.0, 1e-6);
            } else if record.t_upper == 2.0 && record.age_group == *"Age50to120" {
                assert_almost_eq!(record.attack_rate, 1.0 / 3.0, 1e-6);
            } else if record.t_upper == 4.0 && record.age_group == *"Age0to49" {
                assert_almost_eq!(record.attack_rate, 2.0 / 3.0, 1e-6);
            } else if record.t_upper == 4.0 && record.age_group == *"Age50to120" {
                assert_almost_eq!(record.attack_rate, 3.0 / 3.0, 1e-6);
            } else if record.t_upper == 6.0 && record.age_group == *"Age0to49" {
                assert_almost_eq!(record.attack_rate, 2.0 / 3.0, 1e-6);
            } else if record.t_upper == 6.0 && record.age_group == *"Age50to120" {
                assert_almost_eq!(record.attack_rate, 3.0 / 3.0, 1e-6);
            }
        }
        assert_eq!(line_count, 8);
    }
}
