use crate::error::ModelError;
use crate::infectiousness_manager::InfectionData;
use crate::population_loader::{PersonId};
use crate::settings::{Person, SettingCode};
use ixa::prelude::*;
use ixa::profiling::open_span;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct TransmissionReport {
    time: f64,
    target_id: PersonId,
    infected_by: Option<PersonId>,
    infection_setting_id: Option<String>,
}

define_report!(TransmissionReport);

fn record_transmission_event(
    context: &mut Context,
    target_id: PersonId,
    infected_by: Option<PersonId>,
    infection_setting_id: Option<SettingCode>,
) {
    let infection_setting_id = infection_setting_id.map(|code| code.0.to_report_string());
    if infected_by.is_some() {
        context.send_report(TransmissionReport {
            time: context.get_current_time(),
            target_id,
            infected_by,
            infection_setting_id,
        });
    }
}

/// # Errors
///
/// Will return `ModelError` if the report cannot be added
pub fn init(context: &mut Context, file_name: &str) -> Result<(), ModelError> {
    context.add_report::<TransmissionReport>(file_name)?;
    context.subscribe_to_event::<PropertyChangeEvent<Person, InfectionData>>(|context, event| {
        let _span = open_span("transmission_report");
        if let InfectionData::Infectious {
            infected_by,
            infection_setting_id,
            ..
        } = event.current
        {
            record_transmission_event(context, event.entity_id, infected_by, infection_setting_id);
        }
    });
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::population_loader::Person;
    use crate::{
        Age,
        infectiousness_manager::InfectionContextExt,
        parameters::{ContextParametersExt, GlobalParams, Params},
        population_loader::PersonId,
        rate_fns::load_rate_fns,
        reports::ReportParams,
        settings::{Person, SettingCode},
    };
    use ixa::{
        Context, ContextEntitiesExt, ContextGlobalPropertiesExt, ContextRandomExt,
        ContextReportExt, assert_almost_eq, csv, with,
    };
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_context_with_report(transmission_report: ReportParams) -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    max_time: 10.0,
                    transmission_report,
                    ..Default::default()
                },
            )
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    fn test_generate_transmission_report() {
        let mut context = setup_context_with_report(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: None,
        });

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let source: PersonId = context.add_entity(with!(Person, Age(30))).unwrap();
        let target: PersonId = context.add_entity(with!(Person, Age(30))).unwrap();
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
            transmission_report,
            ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = transmission_report.filename {
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
            let record: crate::reports::transmission_report::TransmissionReport = result.unwrap();
            assert_almost_eq!(record.time, infection_time, 0.0);
            assert_eq!(record.target_id, target);
            assert_eq!(record.infected_by.unwrap(), source);
            assert_eq!(
                record.infection_setting_id,
                setting.map(|code| code.0.to_report_string())
            );
            line_count += 1;
        }
        assert_eq!(line_count, 1);
    }
}
