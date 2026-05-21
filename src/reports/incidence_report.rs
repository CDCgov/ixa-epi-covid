use crate::{
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    population_loader::{Age, Person},
    symptom_status_manager::SymptomStatus,
};
use ixa::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyIncidenceReport {
    t_upper: f64,
    age: u8,
    event: String,
    count: u32,
}

define_report!(PersonPropertyIncidenceReport);

pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), ModelError> {
    context.add_report::<PersonPropertyIncidenceReport>(file_name)?;

    // // all possible ages need to be know a head of time
    // let ages: Vec<u8> = (0..=120).collect();
    // let ages_symp_vec = ages.clone();

    // let inf_vec = [InfectionStatus::Infectious, InfectionStatus::Recovered];

    // let symp_vec = [
    //     SymptomStatus::Mild,
    //     SymptomStatus::Severe,
    //     SymptomStatus::Critical,
    //     SymptomStatus::Dead,
    //     SymptomStatus::Resolved,
    // ];

    // this doesn't stop if the simulation does not shutdown elsewhere
    context.track_periodic_value_change_counts::<Person, (Age,), InfectionStatus, _>(
        period,
        move |context, counter| {
            let t_upper = context.get_current_time();
            if t_upper > 0.0 {
                for (stratum, count) in counter.iter() {
                    let (age, infection_status) = stratum;
                    context.send_report(PersonPropertyIncidenceReport {
                        t_upper,
                        age: age.0.0,
                        event: format!("{:?}", infection_status),
                        count: *count as u32,
                    });
                }
            }
        },
    );

    context.track_periodic_value_change_counts::<Person, (Age,), SymptomStatus, _>(
        period,
        move |context, counter| {
            let t_upper = context.get_current_time();
            if t_upper > 0.0 {
                for (stratum, count) in counter.iter() {
                    let (age, symptom_status) = stratum;
                    context.send_report(PersonPropertyIncidenceReport {
                        t_upper,
                        age: age.0.0,
                        event: format!("{:?}", symptom_status),
                        count: *count as u32,
                    });
                }
            }
        },
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        infectiousness_manager::InfectionContextExt,
        parameters::{ContextParametersExt, GlobalParams, OrderedAgeGroupsParam, Params},
        population_loader::{Age, Person, PersonId},
        rate_fns::load_rate_fns,
        reports::ReportParams,
        settings::SettingCode,
        symptom_status_manager::SymptomAgeGroup,
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
        context
            .set_global_property_value(
                OrderedAgeGroupsParam,
                vec![SymptomAgeGroup {
                    label: "Age0To120".to_string(),
                    min: 0,
                    max: 120,
                }],
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
        context.add_plan(3.0, move |context| {
            context.shutdown();
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
        // Only events are included so the line count should match the event count
        assert!(line_count == event_count);
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
        context.add_plan(infection_time - 0.1, move |context| {
            context.set_property::<Person, Age>(target, Age(44));
        });

        context.add_plan(100.0, move |context| {
            context.shutdown();
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

        // Only events are included so the line count should match the event count
        assert!(line_count == event_count);
        assert_eq!(event_count, 1);
    }
}
