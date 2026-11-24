use crate::{
    error::ModelError,
    intervention_manager::{
        AcceptsAndIsolatesCounty, AcceptsAndIsolatesState, AcceptsCommunityMobilityCounty,
        AcceptsCommunityMobilityState, AcceptsMaskMandateCounty, AcceptsMaskMandateState,
        AcceptsSchoolClosureCounty, AcceptsSchoolClosureState, AcceptsWorkMobilityCounty,
        AcceptsWorkMobilityState,
    },
    population_loader::{Age, Alive, Person},
};
use ixa::prelude::*;
use ixa::{ExecutionPhase, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, PartialEq, Debug, Deserialize)]
struct InterventionReport {
    t: f64,
    age: u8,
    isolation_guidance: bool,
    school_closure: bool,
    mask_mandate: bool,
    community_mobility_reduction: bool,
    workplace_mobility_reduction: bool,
    count: usize,
}

define_report!(InterventionReport);
type ReportProperties = (
    Age,
    AcceptsAndIsolatesState,
    AcceptsSchoolClosureState,
    AcceptsMaskMandateState,
    AcceptsCommunityMobilityState,
    AcceptsWorkMobilityState,
    AcceptsAndIsolatesCounty,
    AcceptsSchoolClosureCounty,
    AcceptsMaskMandateCounty,
    AcceptsCommunityMobilityCounty,
    AcceptsWorkMobilityCounty,
);
define_multi_property!(
    Person,
    (
        Age,
        AcceptsAndIsolatesState,
        AcceptsSchoolClosureState,
        AcceptsMaskMandateState,
        AcceptsCommunityMobilityState,
        AcceptsWorkMobilityState,
        AcceptsAndIsolatesCounty,
        AcceptsSchoolClosureCounty,
        AcceptsMaskMandateCounty,
        AcceptsCommunityMobilityCounty,
        AcceptsWorkMobilityCounty
    )
);

struct PropertyReportDataContainer {
    report_map_container: HashMap<ReportProperties, usize>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        report_map_container: HashMap::default()
    }
);
type ReportEvent = PropertyChangeEvent<Person, ReportProperties>;

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
        context.send_report(InterventionReport {
            t: context.get_current_time(),
            age: values.0.0,
            isolation_guidance: values.1.0.is_some() || values.6.0.is_some(),
            school_closure: values.2.0.is_some() || values.7.0.is_some(),
            mask_mandate: values.3.0.is_some() || values.8.0.is_some(),
            community_mobility_reduction: values.4.0.is_some() || values.9.0.is_some(),
            workplace_mobility_reduction: values.5.0.is_some() || values.10.0.is_some(),
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
    context.add_report::<InterventionReport>(file_name)?;

    let mut map_counts = HashMap::default();

    context.with_query_results(with!(Person, Alive(true)), &mut |current_people| {
        //current_people = results.to_owned_vec();
        for person in current_people {
            let value: ReportProperties = context.get_property(person);
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
    use crate::geography::Geography;
    use crate::infectiousness_manager::InfectionData;
    use crate::intervention_manager::{
        Intervention, InterventionContextExt, InterventionModifierSpecification, Modifier,
    };
    use crate::isolation_guidance::time_based_isolation_guidance;
    use crate::parameters::SettingProperties;
    use crate::pop_reader::parser::parse_fips_school_id;
    use crate::population_loader::Person;
    use crate::settings::{Itinerary, SettingCategory};
    use crate::symptom_status_manager::SymptomData;
    use crate::triggers::CustomTrigger;
    use crate::{
        Age,
        parameters::{ContextParametersExt, GlobalParams, Params},
        reports::ReportParams,
        settings::SettingCode,
    };
    use ixa::{HashMap, csv, prelude::*};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }
    fn make_home_id(home_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(home_id).unwrap().1)
    }

    fn setup(intervention_report: ReportParams) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Work, 0.25),
                (SettingCategory::Community, 0.25),
            ]),
            default_modifiers: HashMap::from_iter([
                (
                    Modifier::SchoolClosure,
                    InterventionModifierSpecification::Itinerary {
                        home: None,
                        school: Some([1.0, 0.0, 0.0, 0.0]),
                        work: None,
                        community: None,
                    },
                ),
                (
                    Modifier::WorkplaceMobilityReduction,
                    InterventionModifierSpecification::Itinerary {
                        home: None,
                        school: None,
                        work: Some([1.0, 0.0, 0.0, 0.0]),
                        community: None,
                    },
                ),
                (
                    Modifier::CommunityMobilityReduction,
                    InterventionModifierSpecification::Itinerary {
                        home: None,
                        school: None,
                        work: None,
                        community: Some([1.0, 0.0, 0.0, 0.0]),
                    },
                ),
                (
                    Modifier::MaskMandate,
                    InterventionModifierSpecification::Transmission {
                        infectiousness_multiplier: 0.9,
                        susceptibility_multiplier: 0.9,
                        settings: vec![
                            SettingCategory::Work,
                            SettingCategory::School,
                            SettingCategory::Community,
                        ],
                    },
                ),
                (
                    Modifier::IsolationGuidance,
                    InterventionModifierSpecification::IsolationGuidance {
                        home: None,
                        school: Some([1.0, 0.0, 0.0, 0.0]),
                        work: Some([1.0, 0.0, 0.0, 0.0]),
                        community: Some([1.0, 0.0, 0.0, 0.0]),
                        infectiousness_multiplier: 0.9,
                        susceptibility_multiplier: 0.9,
                        settings: vec![SettingCategory::Home],
                        post_symptoms_isolation_duration: 0.0,
                        minimum_isolation_duration: 2.0,
                        delay_symptoms_to_isolation: 0.0,
                    },
                ),
            ]),
            max_time: 3.0,
            intervention_report,
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_generate_intervention_report() {
        let mut context = setup(ReportParams {
            write: true,
            filename: Some("output.csv".to_string()),
            period: Some(1.0),
        });
        time_based_isolation_guidance::init(&mut context).unwrap();

        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let config = context.report_options();
        config.directory(path.clone());

        let school_code = make_school_id(b"16037960200002");
        let home_code = make_home_id(b"16037960200002");
        let home_code2 = make_home_id(b"1677760200003");
        let community_code = home_code.extract_community();
        let community_code2 = home_code2.extract_community();
        let g1 = Geography::County(home_code.0.state_county_code().unwrap());
        let intervention1 = Intervention {
            modifier: Modifier::IsolationGuidance,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 2.0,
            },
        };

        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_intervention_triggers(intervention1);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.set_property(
            p2,
            Itinerary {
                setting_ids: [
                    Some(home_code2),
                    None,
                    Some(school_code),
                    Some(community_code2),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        // people need to be added to the context before initializing the reports
        crate::reports::init(&mut context).unwrap();

        context.add_plan(2.0, move |context| {
            // person p1 becomes infectious and symptomatic so the isolation will affect them
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Infectious {
                    infection_time: 2.0,
                    infected_by: None,
                    infection_setting_id: None,
                },
            );
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 2.0 });
        });

        context.add_plan(3.0, move |context| {
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Recovered {
                    infection_time: 2.0,
                    recovery_time: 3.0,
                },
            );
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 2.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 3.0,
                },
            );
        });

        context.add_plan(4.0, Context::shutdown);
        context.execute();

        let Params {
            intervention_report,
            ..
        } = context.get_params().clone();
        let file_path = if let Some(name) = intervention_report.filename {
            path.join(name)
        } else {
            panic!("No report name specified");
        };

        assert!(file_path.exists());
        std::mem::drop(context);

        assert!(file_path.exists());
        let mut reader = csv::Reader::from_path(file_path).unwrap();
        let mut line_count = 0;
        let mut isolation_active_count = 0;
        for result in reader.deserialize() {
            let record: crate::reports::intervention_report::InterventionReport = result.unwrap();
            line_count += 1;
            if record.t == 0.0 || record.t == 1.0 {
                if record.age == 10 {
                    assert!(!record.isolation_guidance);
                    assert!(!record.mask_mandate);
                    assert!(!record.school_closure);
                    assert!(!record.workplace_mobility_reduction);
                    assert!(!record.community_mobility_reduction);
                    assert_eq!(record.count, 2);
                } else {
                    panic!("invalid age at t == 0.0")
                }
            } else if record.t == 2.0 || record.t == 3.0 {
                if record.age == 10 {
                    if record.isolation_guidance {
                        isolation_active_count += 1;
                    }
                    assert!(!record.mask_mandate);
                    assert!(!record.school_closure);
                    assert!(!record.workplace_mobility_reduction);
                    assert!(!record.community_mobility_reduction);
                    assert_eq!(record.count, 1);
                } else {
                    panic!("invalid age at t == 2.0")
                }
            } else if record.t == 4.0 {
                if record.age == 10 {
                    if record.isolation_guidance {
                        assert!(!record.mask_mandate);
                        assert!(!record.school_closure);
                        assert!(!record.workplace_mobility_reduction);
                        assert!(!record.community_mobility_reduction);
                        assert_eq!(record.count, 0);
                    } else {
                        assert!(!record.isolation_guidance);
                        assert!(!record.mask_mandate);
                        assert!(!record.school_closure);
                        assert!(!record.workplace_mobility_reduction);
                        assert!(!record.community_mobility_reduction);
                        assert_eq!(record.count, 2);
                    }
                } else {
                    panic!("invalid age at t == 4.0")
                }
            } else {
                panic!("record time {0} is invalid", record.t)
            }
        }

        assert_eq!(line_count, 8);
        assert_eq!(isolation_active_count, 2);
    }
}
