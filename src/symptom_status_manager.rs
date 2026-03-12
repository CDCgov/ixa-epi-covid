use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_property,
    define_rng, impl_property, prelude::PropertyChangeEvent,
};
use rand_distr::LogNormal;
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params,
    infectiousness_manager::InfectionStatus,
    population_loader::{Person, PersonId},
};

define_rng!(SymptomsRng);

#[derive(Serialize, Deserialize, PartialEq, Debug, Copy, Clone, Eq, Hash)]
pub enum SymptomStatus {
    NoSymptoms,
    Mild,
    Severe,
    Critical,
    Resolved,
    Dead,
}

impl_property!(
    SymptomStatus,
    Person,
    default_const = SymptomStatus::NoSymptoms
);

pub enum SymptomAgeGroupNames {
    Young,
    Old
}

define_property!(
    SymptomAgeGroupNames,
    Person,
);

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct SymptomDelayDistLogNormParams {
    pub mu: f64,
    pub sigma: f64,
}

fn plan_symptom_transition(
    context: &mut Context,
    person_id: PersonId,
    next_symptom_status: SymptomStatus,
    delay_params: SymptomDelayDistLogNormParams,
) {
    let delay_dist = LogNormal::new(delay_params.mu, delay_params.sigma).unwrap();
    let transition_time =
        context.get_current_time() + context.sample_distr(SymptomsRng, delay_dist);
    context.add_plan(transition_time, move |context| {
        context.set_property::<Person, SymptomStatus>(person_id, next_symptom_status);
    });
}

fn process_symptom_change_event(
    context: &mut Context,
    event: PropertyChangeEvent<Person, SymptomStatus>,
) {
    let &Params {
        probability_severe_given_mild,
        mild_to_severe_delay,
        mild_to_resolved_delay,
        probability_critical_given_severe,
        severe_to_critical_delay,
        severe_to_resolved_delay,
        probability_dead_given_critical,
        critical_to_dead_delay,
        critical_to_resolved_delay,
        ..
    } = context.get_params();

    match event.current {
        SymptomStatus::Mild => {
            if context.sample_bool(SymptomsRng, probability_severe_given_mild) {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Severe,
                    mild_to_severe_delay,
                );
            } else {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Resolved,
                    mild_to_resolved_delay,
                );
            }
        }
        SymptomStatus::Severe => {
            if context.sample_bool(SymptomsRng, probability_critical_given_severe) {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Critical,
                    severe_to_critical_delay,
                );
            } else {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Resolved,
                    severe_to_resolved_delay,
                );
            }
        }
        SymptomStatus::Critical => {
            if context.sample_bool(SymptomsRng, probability_dead_given_critical) {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Dead,
                    critical_to_dead_delay,
                );
            } else {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Resolved,
                    critical_to_resolved_delay,
                );
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
    
    println!("{:?}", symptom_age_groups.clone());

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current == InfectionStatus::Infectious
                && context.sample_bool(SymptomsRng, probability_mild_given_infect)
            {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Mild,
                    infect_to_mild_delay,
                );
            }
        },
    );

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
            process_symptom_change_event(context, event);
        },
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;
    use std::rc::Rc;
    use super::init;
    use crate::infectiousness_manager::InfectionContextExt;
    use crate::parameters::GlobalParams;
    use crate::population_loader::Person;
    use crate::symptom_status_manager::{SymptomDelayDistLogNormParams, SymptomStatus};
    use crate::{Age, Params};
    use ixa::assert_almost_eq;
    use ixa::prelude::*;

    fn test_proportion(
        current_status: SymptomStatus,
        next_status: SymptomStatus,
        expected_proportion: f64,
    ) {
        // We perform 5000 simulations with 1 person per simulation to check that the proportion of persons
        // who transition from their current symptom status to their next symptom status is close to the expected probability.
        // We use a large number of simulations since the outcome is stochastic.
        let num_sims = 5000;
        let count = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let count_clone = Rc::clone(&count);
            let mut context = Context::new();

            let parameters = match (current_status, next_status) {
                (SymptomStatus::NoSymptoms, SymptomStatus::Mild) => Params {
                    probability_mild_given_infect: expected_proportion,
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Severe) => Params {
                    probability_severe_given_mild: expected_proportion,
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Resolved) => Params {
                    probability_severe_given_mild: 1.0 - expected_proportion,
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Critical) => Params {
                    probability_critical_given_severe: expected_proportion,
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Resolved) => Params {
                    probability_critical_given_severe: 1.0 - expected_proportion,
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Dead) => Params {
                    probability_dead_given_critical: expected_proportion,
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Resolved) => Params {
                    probability_dead_given_critical: 1.0 - expected_proportion,
                    ..Default::default()
                },
                _ => panic!(
                    "Invalid status transition combination: {:?} -> {:?}",
                    current_status, next_status
                ),
            };

            context.init_random(seed);
            context
                .set_global_property_value(GlobalParams, parameters)
                .unwrap();

            // Add our person
            let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
            // Initialize event subscriptions and plans for symptom status manager
            init(&mut context).unwrap();

            // If the next status is NoSymptoms we don't do anything because that transition is not valid
            // If the next status is Mild we must infect the person to trigger the symptom status manager
            // Otherwise we set the person's symptom status to the current status to trigger the symptom status manager
            match next_status {
                SymptomStatus::NoSymptoms => (),
                SymptomStatus::Mild => context.infect_person(p1, None, None, None),
                _ => context.set_property::<Person, SymptomStatus>(p1, current_status),
            }
            // Add a plan to shutdown
            context.add_plan(100.0, ixa::Context::shutdown);

            context.subscribe_to_event(
                move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                    if event.current == next_status && event.previous == current_status {
                        *count_clone.borrow_mut() += 1;
                        context.shutdown();
                    } else {
                        context.shutdown();
                    }
                },
            );
            // Run the simulation
            context.execute();
        }
        // Check that the proportion of people is close to the expected proportion
        assert_almost_eq!(
            *count.borrow() as f64 / (num_sims) as f64,
            expected_proportion,
            0.01
        );
    }

    fn test_duration(
        current_status: SymptomStatus,
        next_status: SymptomStatus,
        expected_mu: f64,
        expected_sigma: f64,
    ) {
        // We run 5000 simulations with 1 person per simulation and get the duration of the delay between the specified symptom statuses.
        // We calculate across simulations the mean of these delays and compare to the mean of the expected distribution.
        // We need to run many simulations to get a precise estimate of the mean.
        let num_sims = 5000;
        let durations = Rc::new(RefCell::new(Vec::new()));
        for seed in 0..num_sims {
            let durations_clone = Rc::clone(&durations);
            let mut context = Context::new();

            let parameters = match (current_status, next_status) {
                (SymptomStatus::NoSymptoms, SymptomStatus::Mild) => Params {
                    probability_mild_given_infect: 1.0,
                    infect_to_mild_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Severe) => Params {
                    probability_severe_given_mild: 1.0,
                    mild_to_severe_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Resolved) => Params {
                    probability_severe_given_mild: 0.0,
                    mild_to_resolved_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Critical) => Params {
                    probability_critical_given_severe: 1.0,
                    severe_to_critical_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Resolved) => Params {
                    probability_critical_given_severe: 0.0,
                    severe_to_resolved_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Dead) => Params {
                    probability_dead_given_critical: 1.0,
                    critical_to_dead_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Resolved) => Params {
                    probability_dead_given_critical: 0.0,
                    critical_to_resolved_delay: SymptomDelayDistLogNormParams {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                _ => panic!(
                    "Invalid status transition combination: {:?} -> {:?}",
                    current_status, next_status
                ),
            };

            context.init_random(seed);
            context
                .set_global_property_value(GlobalParams, parameters)
                .unwrap();

            // Add our person
            let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
            // Initialize event subscriptions and plans for symptom status manager
            init(&mut context).unwrap();

            // If the next status is NoSymptoms we don't do anything because that transition is not valid
            // If the next status is Mild we must infect the person to trigger the symptom status manager
            // Otherwise we set the person's symptom status to the current status to trigger the symptom status manager
            match next_status {
                SymptomStatus::NoSymptoms => (),
                SymptomStatus::Mild => context.infect_person(p1, None, None, None),
                _ => context.set_property::<Person, SymptomStatus>(p1, current_status),
            }
            context.subscribe_to_event(
                move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                    if event.current == next_status && event.previous == current_status {
                        durations_clone
                            .borrow_mut()
                            .push(context.get_current_time());
                        context.shutdown();
                    } else {
                        context.shutdown();
                    }
                },
            );
            // Run the simulation
            context.execute();
        }
        // Check that the average duration is close to the expected duration
        // Ideally this would be a ks test, but rand_distr does not provide a cdf function,
        //so we will just check the average duration is close to the expected duration.
        let average_duration: f64 =
            durations.borrow().iter().sum::<f64>() / durations.borrow().len() as f64;
        assert_almost_eq!(average_duration, expected_mu.exp(), 0.1);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_proportion_infected_to_mild() {
        test_proportion(SymptomStatus::NoSymptoms, SymptomStatus::Mild, 0.3);
    }

    #[test]
    fn test_proportion_mild_to_severe() {
        test_proportion(SymptomStatus::Mild, SymptomStatus::Severe, 0.2);
    }

    #[test]
    fn test_proportion_mild_to_resolved() {
        test_proportion(SymptomStatus::Mild, SymptomStatus::Resolved, 0.8);
    }

    #[test]
    fn test_proportion_severe_to_critical() {
        test_proportion(SymptomStatus::Severe, SymptomStatus::Critical, 0.4);
    }

    #[test]
    fn test_proportion_severe_to_resolved() {
        test_proportion(SymptomStatus::Severe, SymptomStatus::Resolved, 0.3);
    }

    #[test]
    fn test_proportion_critical_to_dead() {
        test_proportion(SymptomStatus::Critical, SymptomStatus::Dead, 0.3);
    }

    #[test]
    fn test_proportion_critical_to_resolved() {
        test_proportion(SymptomStatus::Critical, SymptomStatus::Resolved, 0.3);
    }

    #[test]
    fn test_infection_to_mild_duration() {
        test_duration(SymptomStatus::NoSymptoms, SymptomStatus::Mild, 1.0, 0.1);
    }

    #[test]
    fn test_mild_to_severe_duration() {
        test_duration(SymptomStatus::Mild, SymptomStatus::Severe, 1.0, 0.1);
    }

    #[test]
    fn test_mild_to_recovered_duration() {
        test_duration(SymptomStatus::Mild, SymptomStatus::Resolved, 1.0, 0.1);
    }

    #[test]
    fn test_severe_to_critical_duration() {
        test_duration(SymptomStatus::Severe, SymptomStatus::Critical, 1.0, 0.1);
    }

    #[test]
    fn test_severe_to_resolved_duration() {
        test_duration(SymptomStatus::Severe, SymptomStatus::Resolved, 1.0, 0.1);
    }

    #[test]
    fn test_critical_to_dead_duration() {
        test_duration(SymptomStatus::Critical, SymptomStatus::Dead, 1.0, 0.1);
    }

    #[test]
    fn test_critical_to_resolved_duration() {
        test_duration(SymptomStatus::Critical, SymptomStatus::Resolved, 1.0, 0.1);
    }

    #[test]
    fn test_absorbing_states() {
        // We want to check that infected individuals eventually end up in an absorbing state (No Syptoms, Resolved, or Dead).
        let num_sims: u64 = 5000;
        let mut count_no_symptoms: u64 = 0;
        let mut count_resolved: u64 = 0;
        let mut count_dead: u64 = 0;
        let probability_mild_given_infect = 0.5;
        let probability_severe_given_mild = 0.5;
        let probability_critical_given_severe = 0.5;
        let probability_dead_given_critical = 0.5;
        for seed in 0..num_sims {
            let mut context = Context::new();
            let parameters = Params {
                probability_mild_given_infect,
                probability_severe_given_mild,
                probability_critical_given_severe,
                probability_dead_given_critical,
                ..Default::default()
            };
            context.init_random(seed);
            context
                .set_global_property_value(GlobalParams, parameters)
                .unwrap();

            // Add our person
            let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
            // Initialize event subscriptions and plans for symptom status manager
            init(&mut context).unwrap();
            // Infect the person to trigger the symptom status manager
            context.infect_person(p1, None, None, None);
            // Add a plan to shutdown after we see they progress to an absorbing state
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
