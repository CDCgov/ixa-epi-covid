use ixa::{
    Context, ContextEntitiesExt, ContextGlobalPropertiesExt, ContextRandomExt, IxaError,
    define_rng, impl_derived_property, impl_property, prelude::PropertyChangeEvent,
};
use rand_distr::LogNormal;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, PartialOrd};

use crate::{
    Age, ContextParametersExt, Params, error::ModelError, infectiousness_manager::InfectionStatus,
    parameters::OrderedAgeGroupsParam, population_loader::Person,
};

define_rng!(SymptomsRng);

#[derive(Serialize, Deserialize, PartialEq, Debug, Copy, Clone)]
pub enum SymptomData {
    NoSymptoms,
    Mild {
        mild_time: f64,
    },
    Severe {
        mild_time: f64,
        severe_time: f64,
    },
    Critical {
        mild_time: f64,
        severe_time: f64,
        critical_time: f64,
    },
    Resolved {
        mild_time: f64,
        severe_time: Option<f64>,
        critical_time: Option<f64>,
        resolved_time: f64,
    },
    Dead {
        mild_time: f64,
        severe_time: f64,
        critical_time: f64,
        dead_time: f64,
    },
}

impl_property!(SymptomData, Person, default_const = SymptomData::NoSymptoms);

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum SymptomStatus {
    NoSymptoms,
    Mild,
    Severe,
    Critical,
    Resolved,
    Dead,
}

impl_derived_property!(
    SymptomStatus,
    Person,
    [SymptomData],
    [],
    |symptom_data| match symptom_data {
        SymptomData::NoSymptoms => SymptomStatus::NoSymptoms,
        SymptomData::Mild { .. } => SymptomStatus::Mild,
        SymptomData::Severe { .. } => SymptomStatus::Severe,
        SymptomData::Critical { .. } => SymptomStatus::Critical,
        SymptomData::Resolved { .. } => SymptomStatus::Resolved,
        SymptomData::Dead { .. } => SymptomStatus::Dead,
    }
);

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct SymptomAgeGroup {
    pub label: String,
    pub min: u8,
    pub max: u8,
}

impl Ord for SymptomAgeGroup {
    fn cmp(&self, other: &Self) -> Ordering {
        self.min.cmp(&other.min)
    }
}

impl PartialOrd for SymptomAgeGroup {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Serialize, PartialEq, Copy, Clone)]
pub struct AgeGroupIndex(pub usize);

impl_derived_property!(
    AgeGroupIndex,
    Person,
    [Age],
    [OrderedAgeGroupsParam],
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
        return AgeGroupIndex(index);
    }
);

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct SymptomDelayDistLogNormParams {
    pub mu: f64,
    pub sigma: f64,
}

impl SymptomDelayDistLogNormParams {
    pub fn validate(&self, param_name: &str) -> Result<(), ModelError> {
        if self.sigma < 0.0 {
            Err(ModelError::ModelError(format!(
                "sigma value for {} is {}; log normal distribution sigmas must be non-negative.",
                param_name, self.sigma
            )))
        } else {
            Ok(())
        }
    }
}

fn draw_transition_time(context: &mut Context, delay_params: SymptomDelayDistLogNormParams) -> f64 {
    let delay_dist = LogNormal::new(delay_params.mu, delay_params.sigma).unwrap();
    context.get_current_time() + context.sample_distr(SymptomsRng, delay_dist)
}

fn process_symptom_change_event(
    context: &mut Context,
    event: PropertyChangeEvent<Person, SymptomData>,
) {
    let &Params {
        ref probability_severe_given_mild,
        mild_to_severe_delay,
        mild_to_resolved_delay,
        ref probability_critical_given_severe,
        severe_to_critical_delay,
        severe_to_resolved_delay,
        ref probability_dead_given_critical,
        critical_to_dead_delay,
        critical_to_resolved_delay,
        ..
    } = context.get_params();

    let ordered_symptom_age_groups = context
        .get_global_property_value(OrderedAgeGroupsParam)
        .unwrap();

    let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(event.entity_id);
    let symptom_age_group_label = &ordered_symptom_age_groups[symptom_age_group_index.0].label;

    match event.current {
        SymptomData::Mild { .. } => {
            if context.sample_bool(
                SymptomsRng,
                *probability_severe_given_mild
                    .get(symptom_age_group_label)
                    .unwrap(),
            ) {
                let transition_time = draw_transition_time(context, mild_to_severe_delay);
                let SymptomData::Mild { mild_time } =
                    context.get_property::<Person, SymptomData>(event.entity_id)
                else {
                    panic!("Person is not in Mild.")
                };
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Severe {
                            mild_time,
                            severe_time: transition_time,
                        },
                    );
                });
            } else {
                let transition_time = draw_transition_time(context, mild_to_resolved_delay);
                let SymptomData::Mild { mild_time } =
                    context.get_property::<Person, SymptomData>(event.entity_id)
                else {
                    panic!("Person is not in Mild.")
                };
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Resolved {
                            mild_time,
                            severe_time: None,
                            critical_time: None,
                            resolved_time: transition_time,
                        },
                    );
                });
            }
        }
        SymptomData::Severe { .. } => {
            if context.sample_bool(
                SymptomsRng,
                *probability_critical_given_severe
                    .get(symptom_age_group_label)
                    .unwrap(),
            ) {
                let transition_time = draw_transition_time(context, severe_to_critical_delay);
                let SymptomData::Severe {
                    mild_time,
                    severe_time,
                } = context.get_property::<Person, SymptomData>(event.entity_id)
                else {
                    panic!("Person is not in Severe.")
                };
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Critical {
                            mild_time,
                            severe_time,
                            critical_time: transition_time,
                        },
                    );
                });
            } else {
                let transition_time = draw_transition_time(context, severe_to_resolved_delay);
                let SymptomData::Severe {
                    mild_time,
                    severe_time,
                } = context.get_property::<Person, SymptomData>(event.entity_id)
                else {
                    panic!("Person is not in Severe.")
                };
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Resolved {
                            mild_time,
                            severe_time: Some(severe_time),
                            critical_time: None,
                            resolved_time: transition_time,
                        },
                    );
                });
            }
        }
        SymptomData::Critical { .. } => {
            if context.sample_bool(
                SymptomsRng,
                *probability_dead_given_critical
                    .get(symptom_age_group_label)
                    .unwrap(),
            ) {
                let transition_time = draw_transition_time(context, critical_to_dead_delay);
                let SymptomData::Critical {
                    mild_time,
                    severe_time,
                    critical_time,
                } = context.get_property::<Person, SymptomData>(event.entity_id)
                else {
                    panic!("Person is not in Critical.")
                };
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Dead {
                            mild_time,
                            severe_time,
                            critical_time,
                            dead_time: transition_time,
                        },
                    );
                });
            } else {
                let transition_time = draw_transition_time(context, critical_to_resolved_delay);
                let SymptomData::Critical {
                    mild_time,
                    severe_time,
                    critical_time,
                } = context.get_property::<Person, SymptomData>(event.entity_id)
                else {
                    panic!("Person is not in Severe.")
                };
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Resolved {
                            mild_time,
                            severe_time: Some(severe_time),
                            critical_time: Some(critical_time),
                            resolved_time: transition_time,
                        },
                    );
                });
            }
        }
        _ => (),
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        probability_mild_given_infect,
        infect_to_mild_delay,
        ..
    } = context.get_params();

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current == InfectionStatus::Infectious
                && context.sample_bool(SymptomsRng, probability_mild_given_infect)
            {
                let transition_time = draw_transition_time(context, infect_to_mild_delay);
                context.add_plan(transition_time, move |context| {
                    context.set_property::<Person, SymptomData>(
                        event.entity_id,
                        SymptomData::Mild {
                            mild_time: transition_time,
                        },
                    );
                });
            }
        },
    );

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomData>| {
            process_symptom_change_event(context, event);
        },
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use super::init;
    use crate::infectiousness_manager::InfectionContextExt;
    use crate::parameters::{GlobalParams, OrderedAgeGroupsParam};
    use crate::population_loader::{Person, PersonId};
    use crate::symptom_status_manager::{
        AgeGroupIndex, SymptomAgeGroup, SymptomData, SymptomDelayDistLogNormParams, SymptomStatus,
        draw_transition_time, process_symptom_change_event,
    };
    use crate::{Age, Params};
    use ixa::assert_almost_eq;
    use ixa::prelude::*;

    #[test]
    fn test_validate_log_norm_params() {
        let log_normal_delay_params = SymptomDelayDistLogNormParams {
            mu: 1.0,
            sigma: -2.0,
        };
        let result = log_normal_delay_params.validate("param_name");
        assert!(result.is_err())
    }

    #[test]
    fn test_age_group_property_derivation() {
        let ordered_symptom_age_groups: Vec<SymptomAgeGroup> = vec![
            SymptomAgeGroup {
                label: "Age0To17".to_string(),
                min: 0,
                max: 17,
            },
            SymptomAgeGroup {
                label: "Age18To49".to_string(),
                min: 18,
                max: 49,
            },
            SymptomAgeGroup {
                label: "Age50To64".to_string(),
                min: 50,
                max: 64,
            },
            SymptomAgeGroup {
                label: "Age65Plus".to_string(),
                min: 65,
                max: 120,
            },
        ];
        let mut context = Context::new();
        context.init_random(1234);
        context
            .set_global_property_value(OrderedAgeGroupsParam, ordered_symptom_age_groups)
            .unwrap();

        // test people who are 0, 17, 18, 49, 50, 64, 65, and 100 years old for correct derived person property
        let person_id: PersonId = context.add_entity((Age(0),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(0));

        let person_id: PersonId = context.add_entity((Age(17),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(0));

        let person_id: PersonId = context.add_entity((Age(18),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(1));

        let person_id: PersonId = context.add_entity((Age(49),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(1));

        let person_id: PersonId = context.add_entity((Age(50),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(2));

        let person_id: PersonId = context.add_entity((Age(64),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(2));

        let person_id: PersonId = context.add_entity((Age(65),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(3));

        let person_id: PersonId = context.add_entity((Age(100),)).unwrap();
        let symptom_age_group_index = context.get_property::<Person, AgeGroupIndex>(person_id);
        assert_eq!(symptom_age_group_index, AgeGroupIndex(3));
    }

    #[test]
    fn test_draw_transition_time() {
        let start_time = 3.0;
        let state_duration: f64 = 7.0;
        let log_normal_delay_params = SymptomDelayDistLogNormParams {
            mu: state_duration.ln(),
            sigma: 0.0,
        };
        let mut context = Context::new();
        context.set_start_time(start_time);
        let transition_time = draw_transition_time(&mut context, log_normal_delay_params);
        assert_eq!(transition_time, start_time + state_duration)
    }

    // most of the substantive logic in this module is in process_symptom_change_event;
    // to test the logic of this function as used in a closure when subscribed to SymptomData changes,
    // we want simulated people who will follow each of the following trajectories
    // mild -> resolved
    // mild -> severe -> resolved
    // mild -> severe -> critical -> resolved
    // mild -> severe -> critical -> dead
    // each trajectory is defined by a series of transition probability values
    // here, all probabilities are either 0.0 or 1.0, so the test is deterministic
    // conveniently, we have four age categories, so we can make simulated people have different ages,
    // with each age coresponding to a different trajectory; this also serves to test that the function
    // is getting the correct probabilities for each age category
    // we use unique durations for each symptom to help test that the timing of events is as expected
    #[test]
    fn test_age_specific_symptom_trajectories() {
        let mild_to_severe_duration: f64 = 1.0;
        let mild_to_resolved_duration: f64 = 2.0;
        let severe_to_critical_duration: f64 = 3.0;
        let severe_to_resolved_duration: f64 = 4.0;
        let critical_to_dead_duration: f64 = 5.0;
        let critical_to_resolved_duration: f64 = 6.0;
        let mut context = Context::new();
        let parameters = Params {
            probability_severe_given_mild: ixa::HashMap::from_iter([
                ("Age0To17".to_string(), 0.0),
                ("Age18To49".to_string(), 1.0),
                ("Age50To64".to_string(), 1.0),
                ("Age65Plus".to_string(), 1.0),
            ]),
            mild_to_severe_delay: SymptomDelayDistLogNormParams {
                mu: mild_to_severe_duration.ln(),
                sigma: 0.0,
            },
            mild_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: mild_to_resolved_duration.ln(),
                sigma: 0.0,
            },
            probability_critical_given_severe: ixa::HashMap::from_iter([
                ("Age0To17".to_string(), 0.0),
                ("Age18To49".to_string(), 0.0),
                ("Age50To64".to_string(), 1.0),
                ("Age65Plus".to_string(), 1.0),
            ]),
            severe_to_critical_delay: SymptomDelayDistLogNormParams {
                mu: severe_to_critical_duration.ln(),
                sigma: 0.0,
            },
            severe_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: severe_to_resolved_duration.ln(),
                sigma: 0.0,
            },
            probability_dead_given_critical: ixa::HashMap::from_iter([
                ("Age0To17".to_string(), 0.0),
                ("Age18To49".to_string(), 0.0),
                ("Age50To64".to_string(), 0.0),
                ("Age65Plus".to_string(), 1.0),
            ]),
            critical_to_dead_delay: SymptomDelayDistLogNormParams {
                mu: critical_to_dead_duration.ln(),
                sigma: 0.0,
            },
            critical_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: critical_to_resolved_duration.ln(),
                sigma: 0.0,
            },
            ..Default::default()
        };
        let ordered_symptom_age_groups: Vec<SymptomAgeGroup> = vec![
            SymptomAgeGroup {
                label: "Age0To17".to_string(),
                min: 0,
                max: 17,
            },
            SymptomAgeGroup {
                label: "Age18To49".to_string(),
                min: 18,
                max: 49,
            },
            SymptomAgeGroup {
                label: "Age50To64".to_string(),
                min: 50,
                max: 64,
            },
            SymptomAgeGroup {
                label: "Age65Plus".to_string(),
                min: 65,
                max: 120,
            },
        ];
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();
        context
            .set_global_property_value(OrderedAgeGroupsParam, ordered_symptom_age_groups)
            .unwrap();
        let p1 = context.add_entity::<Person, _>((Age(10),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let p3 = context.add_entity::<Person, _>((Age(60),)).unwrap();
        let p4 = context.add_entity::<Person, _>((Age(80),)).unwrap();

        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomData>| {
                process_symptom_change_event(context, event);
            },
        );
        for person in [p1, p2, p3, p4] {
            context.set_property::<Person, SymptomData>(
                person,
                SymptomData::Mild {
                    mild_time: context.get_current_time(),
                },
            );
        }
        context.execute();

        assert_eq!(
            // mild -> resolved
            context.get_property::<Person, SymptomData>(p1),
            SymptomData::Resolved {
                mild_time: 0.0,
                severe_time: None,
                critical_time: None,
                resolved_time: 0.0 + mild_to_resolved_duration,
            }
        );
        assert_eq!(
            // mild -> severe -> resolved
            context.get_property::<Person, SymptomData>(p2),
            SymptomData::Resolved {
                mild_time: 0.0,
                severe_time: Some(0.0 + mild_to_severe_duration),
                critical_time: None,
                resolved_time: 0.0 + mild_to_severe_duration + severe_to_resolved_duration,
            }
        );
        assert_eq!(
            // mild -> severe -> critical -> resolved
            context.get_property::<Person, SymptomData>(p3),
            SymptomData::Resolved {
                mild_time: 0.0,
                severe_time: Some(0.0 + mild_to_severe_duration),
                critical_time: Some(0.0 + mild_to_severe_duration + severe_to_critical_duration),
                resolved_time: 0.0
                    + mild_to_severe_duration
                    + severe_to_critical_duration
                    + critical_to_resolved_duration,
            }
        );
        assert_eq!(
            // mild -> severe -> critical -> dead
            context.get_property::<Person, SymptomData>(p4),
            SymptomData::Dead {
                mild_time: 0.0,
                severe_time: 0.0 + mild_to_severe_duration,
                critical_time: 0.0 + mild_to_severe_duration + severe_to_critical_duration,
                dead_time: 0.0
                    + mild_to_severe_duration
                    + severe_to_critical_duration
                    + critical_to_dead_duration,
            }
        );
    }

    // testing NoSymptoms to Mild is different because the trigger is an infection
    // and this is coded in the init, rather than using process_symptom_change_event,
    // although process_symptom_change_event is later called in the init
    #[test]
    fn test_no_symptoms_to_mild() {
        let infect_to_mild_duration: f64 = 1.0;
        let mut context = Context::new();
        let parameters = Params {
            probability_mild_given_infect: 1.0,
            infect_to_mild_delay: SymptomDelayDistLogNormParams {
                mu: infect_to_mild_duration.ln(),
                sigma: 0.0,
            },
            probability_severe_given_mild: ixa::HashMap::from_iter([(
                "Age0To120".to_string(),
                0.0,
            )]),
            // some probability_severe_given_mild needs to be specified because mild -> severe or mild -> resolved
            // is *scheduled* the moment the person goes from no symptoms to mild, even though
            // the simulation is shut down before the person actually leaves the mild state
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
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

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        init(&mut context).unwrap();
        context.infect_person(p1, None, None);
        context.add_plan_with_phase(
            infect_to_mild_duration,
            ixa::Context::shutdown,
            ixa::ExecutionPhase::Last,
        );
        context.execute();

        assert_eq!(
            // no symptoms -> mild -> resolved
            context.get_property::<Person, SymptomData>(p1),
            SymptomData::Mild {
                mild_time: infect_to_mild_duration
            }
        );
    }

    #[test]
    fn test_never_symptomatic() {
        let mut context = Context::new();
        let parameters = Params {
            probability_mild_given_infect: 0.0,
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        init(&mut context).unwrap();
        context.infect_person(p1, None, None);
        context.execute();
        assert_eq!(
            context.get_property::<Person, SymptomData>(p1),
            SymptomData::NoSymptoms
        );
    }

    #[test]
    fn test_absorbing_states() {
        // We want to check that infected individuals eventually end up in an absorbing state (No Symptoms, Resolved, or Dead).
        let num_sims: u64 = 5000;
        let mut count_no_symptoms: u64 = 0;
        let mut count_resolved: u64 = 0;
        let mut count_dead: u64 = 0;
        let probability_mild_given_infect = 0.8;
        let probability_severe_given_mild = 0.6;
        let probability_critical_given_severe = 0.3;
        let probability_dead_given_critical = 0.1;
        for seed in 0..num_sims {
            let mut context = Context::new();
            let parameters = Params {
                probability_mild_given_infect,
                probability_severe_given_mild: ixa::HashMap::from_iter([(
                    "Age0To120".to_string(),
                    probability_severe_given_mild,
                )]),
                probability_critical_given_severe: ixa::HashMap::from_iter([(
                    "Age0To120".to_string(),
                    probability_critical_given_severe,
                )]),
                probability_dead_given_critical: ixa::HashMap::from_iter([(
                    "Age0To120".to_string(),
                    probability_dead_given_critical,
                )]),
                ..Default::default()
            };
            context.init_random(seed);
            context
                .set_global_property_value(GlobalParams, parameters)
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

            // Add our person
            let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
            // Initialize event subscriptions and plans for symptom status manager
            init(&mut context).unwrap();
            // Infect the person to trigger the symptom status manager
            context.infect_person(p1, None, None);
            // Add a plan to shutdown after long period once everyone should have progressed to an absorbing state
            context.add_plan(1000.0, ixa::Context::shutdown);

            context.execute();
            let final_status = context.get_property::<Person, SymptomStatus>(p1);
            match final_status {
                SymptomStatus::NoSymptoms => count_no_symptoms += 1,
                SymptomStatus::Resolved => count_resolved += 1,
                SymptomStatus::Dead => count_dead += 1,
                _ => panic!("Person ended in non-absorbing state: {:?}", final_status),
            }
        }
        assert_eq!(count_no_symptoms + count_resolved + count_dead, num_sims);

        assert_almost_eq!(
            count_no_symptoms as f64 / num_sims as f64,
            1.0 - probability_mild_given_infect,
            0.05
        );
        assert_almost_eq!(
            count_resolved as f64 / num_sims as f64,
            probability_mild_given_infect * (1.0 - probability_severe_given_mild)
                + probability_mild_given_infect
                    * probability_severe_given_mild
                    * (1.0 - probability_critical_given_severe)
                + probability_mild_given_infect
                    * probability_severe_given_mild
                    * probability_critical_given_severe
                    * (1.0 - probability_dead_given_critical),
            0.05
        );
        assert_almost_eq!(
            count_dead as f64 / num_sims as f64,
            probability_mild_given_infect
                * probability_severe_given_mild
                * probability_critical_given_severe
                * probability_dead_given_critical,
            0.05
        );
    }
}
