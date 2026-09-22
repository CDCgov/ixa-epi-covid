use crate::{
    error::ModelError, population_loader::Person, symptom_status_manager::HospitalizationStatus,
};
use ixa::ExecutionPhase;
use ixa::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct CurrentHospitalizationsReport {
    t_upper: f64,
    current_hospitalizations: f64,
}

define_report!(CurrentHospitalizationsReport);

struct HospitalizationReportDataContainer {
    hospitalizations: usize,
}

define_data_plugin!(
    HospitalizationReportDataPlugin,
    HospitalizationReportDataContainer,
    HospitalizationReportDataContainer {
        hospitalizations: 0,
    }
);

type ReportEvent = PropertyChangeEvent<Person, HospitalizationStatus>;

fn update_change_counts(context: &mut Context, event: ReportEvent) {
    let report_container_mut = context.get_data_mut(HospitalizationReportDataPlugin);
    if event.current == HospitalizationStatus::Hospitalized {
        report_container_mut.hospitalizations += 1;
    }
    if event.previous == HospitalizationStatus::Hospitalized
        && report_container_mut.hospitalizations > 0
    {
        report_container_mut.hospitalizations -= 1;
    }
}

fn send_property_counts(context: &mut Context) {
    let report_container = context.get_data(HospitalizationReportDataPlugin);
    let current_hospitalizations = report_container.hospitalizations as f64;
    context.send_report(CurrentHospitalizationsReport {
        t_upper: context.get_current_time(),
        current_hospitalizations,
    });
}

pub fn init(context: &mut Context, file_name: &str, period: f64) -> Result<(), ModelError> {
    context.add_report::<CurrentHospitalizationsReport>(file_name)?;

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
    use crate::symptom_status_manager::SymptomData;
    use crate::{
        Age,
        parameters::{ContextParametersExt, GlobalParams, Params},
        rate_fns::load_rate_fns,
        reports::ReportParams,
    };
    use ixa::{csv, prelude::*};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(current_hospitalizations_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 3.0,
                    current_hospitalizations_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_generate_current_hospitalizations_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: Some(3.0),
        });

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let p1 = context.add_entity(with!(Person, Age(42))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(43))).unwrap();
        let p3 = context.add_entity(with!(Person, Age(44))).unwrap();

        crate::reports::init(&mut context).unwrap();

        context.add_plan(0.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Severe {
                    mild_time: 0.0,
                    severe_time: 0.0,
                },
            );
        });
        // this should not count as a new person entering the hospital
        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Critical {
                    mild_time: 0.0,
                    severe_time: 0.0,
                    critical_time: 1.0,
                },
            );
        });

        context.add_plan(2.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p2,
                SymptomData::Severe {
                    mild_time: 0.0,
                    severe_time: 2.0,
                },
            );
        });
        context.add_plan(3.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p3,
                SymptomData::Severe {
                    mild_time: 0.0,
                    severe_time: 3.0,
                },
            );
        });

        context.add_plan(5.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 0.0,
                    severe_time: Some(0.0),
                    critical_time: Some(1.0),
                    resolved_time: 5.0,
                },
            );
            context.set_property::<Person, SymptomData>(
                p2,
                SymptomData::Resolved {
                    mild_time: 0.0,
                    severe_time: Some(2.0),
                    critical_time: None,
                    resolved_time: 5.0,
                },
            );
            context.set_property::<Person, SymptomData>(
                p3,
                SymptomData::Resolved {
                    mild_time: 0.0,
                    severe_time: Some(3.0),
                    critical_time: None,
                    resolved_time: 5.0,
                },
            );
        });

        context.add_plan(7.0, Context::shutdown);
        context.execute();

        let Params {
            current_hospitalizations_report,
            ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = current_hospitalizations_report.filename {
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
            let record: crate::reports::current_hospitalizations_report::CurrentHospitalizationsReport = result.unwrap();
            line_count += 1;
            println!("{:?}", record);
            if record.t_upper == 0.0 {
                // The current hospitalizations are
                // 0 -> 1, 1->1, 2->2,
                assert_eq!(record.current_hospitalizations, 1.0);
            } else if record.t_upper == 3.0 {
                // The current hospitalizations are
                // 3 -> 3, 4 -> 3, 5 -> 0
                assert_eq!(record.current_hospitalizations, 3.0);
            } else if record.t_upper == 6.0 {
                // The current hospitalizations are
                // 6 -> 0, 7 -> 0
                assert_eq!(record.current_hospitalizations, 0.0);
            } else {
                panic!("record times other than 0.0, 3.0, 6.0 are invalid")
            }
        }
        assert_eq!(line_count, 3);
    }
}
