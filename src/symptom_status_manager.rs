use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_rng, impl_property,
    prelude::PropertyChangeEvent,
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

fn plan_symptom_transition(
    context: &mut Context,
    person_id: PersonId,
    next_status: SymptomStatus,
    delay_mu: f64,
    delay_sigma: f64,
) {
    let delay_dist = LogNormal::new(delay_mu, delay_sigma).unwrap();
    let transition_time =
        context.get_current_time() + context.sample_distr(SymptomsRng, delay_dist);
    context.add_plan(transition_time, move |context| {
        context.set_property::<Person, SymptomStatus>(person_id, next_status);
    });
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params {
        infected_to_mild,
        mild_to_severe,
        mild_to_resolved,
        severe_to_critical,
        severe_to_resolved,
        critical_to_dead,
        critical_to_resolved,
        ..
    } = context.get_params().clone();

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current == InfectionStatus::Infectious
                && context.sample_bool(SymptomsRng, infected_to_mild.probability)
            {
                plan_symptom_transition(
                    context,
                    event.entity_id,
                    SymptomStatus::Mild,
                    infected_to_mild.mu,
                    infected_to_mild.sigma,
                );
            }
        },
    );

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| match event.current {
            SymptomStatus::Mild => {
                if context.sample_bool(SymptomsRng, mild_to_severe.probability) {
                    plan_symptom_transition(
                        context,
                        event.entity_id,
                        SymptomStatus::Severe,
                        mild_to_severe.mu,
                        mild_to_severe.sigma,
                    );
                } else {
                    plan_symptom_transition(
                        context,
                        event.entity_id,
                        SymptomStatus::Resolved,
                        mild_to_resolved.mu,
                        mild_to_resolved.sigma,
                    );
                }
            }
            SymptomStatus::Severe => {
                if context.sample_bool(SymptomsRng, severe_to_critical.probability) {
                    plan_symptom_transition(
                        context,
                        event.entity_id,
                        SymptomStatus::Critical,
                        severe_to_critical.mu,
                        severe_to_critical.sigma,
                    );
                } else {
                    plan_symptom_transition(
                        context,
                        event.entity_id,
                        SymptomStatus::Resolved,
                        severe_to_resolved.mu,
                        severe_to_resolved.sigma,
                    );
                }
            }
            SymptomStatus::Critical => {
                if context.sample_bool(SymptomsRng, critical_to_dead.probability) {
                    plan_symptom_transition(
                        context,
                        event.entity_id,
                        SymptomStatus::Dead,
                        critical_to_dead.mu,
                        critical_to_dead.sigma,
                    );
                } else {
                    plan_symptom_transition(
                        context,
                        event.entity_id,
                        SymptomStatus::Resolved,
                        critical_to_resolved.mu,
                        critical_to_resolved.sigma,
                    );
                }
            }
            _ => (),
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
    use crate::parameters::{
        CriticalToDeadParameters, CriticalToResolvedParameters, GlobalParams,
        InfectedToMildParameters, MildToResolvedParameters, MildToSevereParameters,
        SevereToCriticalParameters, SevereToResolvedParameters,
    };
    use crate::population_loader::Person;
    use crate::symptom_status_manager::SymptomStatus;
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
                    infected_to_mild: InfectedToMildParameters {
                        probability: expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Severe) => Params {
                    mild_to_severe: MildToSevereParameters {
                        probability: expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Resolved) => Params {
                    mild_to_severe: MildToSevereParameters {
                        probability: 1.0 - expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    mild_to_resolved: MildToResolvedParameters {
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Critical) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        probability: expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Resolved) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        probability: 1.0 - expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    severe_to_resolved: SevereToResolvedParameters {
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Dead) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        probability: expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Resolved) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        probability: 1.0 - expected_proportion,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    critical_to_resolved: CriticalToResolvedParameters {
                        mu: 1.0,
                        sigma: 0.1,
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
                    infected_to_mild: InfectedToMildParameters {
                        probability: 1.0,
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Severe) => Params {
                    mild_to_severe: MildToSevereParameters {
                        probability: 1.0,
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Resolved) => Params {
                    mild_to_severe: MildToSevereParameters {
                        probability: 0.0,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    mild_to_resolved: MildToResolvedParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Critical) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        probability: 1.0,
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Resolved) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        probability: 0.0,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    severe_to_resolved: SevereToResolvedParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Dead) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        probability: 1.0,
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Resolved) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        probability: 0.0,
                        mu: 1.0,
                        sigma: 0.1,
                    },
                    critical_to_resolved: CriticalToResolvedParameters {
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
        let infected_to_mild: InfectedToMildParameters = InfectedToMildParameters {
            probability: 0.5,
            mu: 1.0,
            sigma: 0.1,
        };
        let mild_to_severe: MildToSevereParameters = MildToSevereParameters {
            probability: 0.5,
            mu: 1.0,
            sigma: 0.1,
        };
        let severe_to_critical: SevereToCriticalParameters = SevereToCriticalParameters {
            probability: 0.5,
            mu: 1.0,
            sigma: 0.1,
        };
        let critical_to_dead: CriticalToDeadParameters = CriticalToDeadParameters {
            probability: 0.5,
            mu: 1.0,
            sigma: 0.1,
        };
        for seed in 0..num_sims {
            let mut context = Context::new();
            let parameters = Params {
                infected_to_mild,
                mild_to_severe,
                severe_to_critical,
                critical_to_dead,
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
            1.0 - infected_to_mild.probability,
            0.05
        );
        assert_almost_eq!(
            count_resolved as f64 / num_sims as f64,
            infected_to_mild.probability * (1.0 - mild_to_severe.probability)
                + infected_to_mild.probability
                    * mild_to_severe.probability
                    * (1.0 - severe_to_critical.probability)
                + infected_to_mild.probability
                    * mild_to_severe.probability
                    * severe_to_critical.probability
                    * (1.0 - critical_to_dead.probability),
            0.05
        );
        assert_almost_eq!(
            count_dead as f64 / num_sims as f64,
            infected_to_mild.probability
                * mild_to_severe.probability
                * severe_to_critical.probability
                * critical_to_dead.probability,
            0.05
        );
    }
}

// All persons should end up with a SymptomStatus of NoSymptoms, Dead, or Resolved.

// TODO:
// -set up parameters (essential to make it run) [DONE]
// -add SymptomStatusData or some other mechanism to track what most symptom statuses a person had, even after resolved (and maybe track timing, too)
// -make probabilities age (category) specific
// -finish validation for parameters
