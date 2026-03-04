use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_rng, impl_property,
    prelude::PropertyChangeEvent,
};
use rand_distr::LogNormal;
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params,
    infectiousness_manager::InfectionStatus,
    population_loader::{AgeGroup, Person},
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub struct SymptomStatusAgeGroup {
    pub min: u8,
    pub probability: f64,
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params {
        infect_to_mild,
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
            let age_group = context.get_property::<Person, AgeGroup>(event.entity_id).0 as usize;
            if event.current == InfectionStatus::Infectious
                && context.sample_bool(
                    SymptomsRng,
                    infect_to_mild.age_groups[age_group].probability,
                )
            {
                let infect_to_mild =
                    LogNormal::new(infect_to_mild.mu, infect_to_mild.sigma).unwrap();
                let mild_time =
                    context.get_current_time() + context.sample_distr(SymptomsRng, infect_to_mild);
                context.add_plan(mild_time, move |context| {
                    context.set_property(event.entity_id, SymptomStatus::Mild);
                });
            }
        },
    );

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| match event.current {
            SymptomStatus::Mild => {
                let age_group =
                    context.get_property::<Person, AgeGroup>(event.entity_id).0 as usize;
                if context.sample_bool(
                    SymptomsRng,
                    mild_to_severe
                        .age_groups
                        .get(age_group)
                        .unwrap()
                        .probability,
                ) {
                    let mild_to_severe =
                        LogNormal::new(mild_to_severe.mu, mild_to_severe.sigma).unwrap();
                    let severe_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, mild_to_severe);
                    context.add_plan(severe_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Severe);
                    });
                } else {
                    let mild_to_resolved =
                        LogNormal::new(mild_to_resolved.mu, mild_to_resolved.sigma).unwrap();
                    let resolved_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, mild_to_resolved);
                    context.add_plan(resolved_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Resolved);
                    });
                }
            }
            SymptomStatus::Severe => {
                let age_group =
                    context.get_property::<Person, AgeGroup>(event.entity_id).0 as usize;
                if context.sample_bool(
                    SymptomsRng,
                    severe_to_critical
                        .age_groups
                        .get(age_group)
                        .unwrap()
                        .probability,
                ) {
                    let severe_to_critical =
                        LogNormal::new(severe_to_critical.mu, severe_to_critical.sigma).unwrap();
                    let critical_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, severe_to_critical);
                    context.add_plan(critical_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Critical);
                    });
                } else {
                    let severe_to_resolved =
                        LogNormal::new(severe_to_resolved.mu, severe_to_resolved.sigma).unwrap();
                    let resolved_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, severe_to_resolved);
                    context.add_plan(resolved_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Resolved);
                    });
                }
            }
            SymptomStatus::Critical => {
                let age_group =
                    context.get_property::<Person, AgeGroup>(event.entity_id).0 as usize;
                if context.sample_bool(
                    SymptomsRng,
                    critical_to_dead
                        .age_groups
                        .get(age_group)
                        .unwrap()
                        .probability,
                ) {
                    let critical_to_dead =
                        LogNormal::new(critical_to_dead.mu, critical_to_dead.sigma).unwrap();
                    let dead_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, critical_to_dead);
                    context.add_plan(dead_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Dead);
                    });
                } else {
                    let critical_to_resolved =
                        LogNormal::new(critical_to_resolved.mu, critical_to_resolved.sigma)
                            .unwrap();
                    let resolved_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, critical_to_resolved);
                    context.add_plan(resolved_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Resolved);
                    });
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
        InfectToMildParameters, MildToResolvedParameters, MildToSevereParameters,
        SevereToCriticalParameters, SevereToResolvedParameters,
    };
    use crate::population_loader::Person;
    use crate::symptom_status_manager::{SymptomStatus, SymptomStatusAgeGroup};
    use crate::{Age, Params};
    use ixa::assert_almost_eq;
    use ixa::prelude::*;

    #[allow(dead_code)]
    fn check_ks_stat(times: &mut [f64], distribution: impl Fn(f64) -> f64) {
        // Sort the empirical times to make an empirical CDF.
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // KS stat is the maximum observed CDF deviation.
        let ks_stat = times
            .iter()
            .enumerate()
            .map(|(i, time)| {
                #[allow(clippy::cast_precision_loss)]
                let empirical_cdf_value = (i as f64) / (times.len() as f64);
                let theoretical_cdf_value = distribution(*time);
                (empirical_cdf_value - theoretical_cdf_value).abs()
            })
            .reduce(f64::max)
            .unwrap();

        assert_almost_eq!(ks_stat, 0.0, 0.01);
    }

    fn test_proportion(
        current_status: SymptomStatus,
        next_status: SymptomStatus,
        expected_proportion: f64,
    ) {
        // We start with 1 person and simulate infecting them 5000 times, we should see that the proportion of time
        // that they have mild symptoms is close to the expected probability of mild symptoms given infection.
        // We use a large number of simulations to ensure that we have enough mild cases to analyze, since the outcome is stochastic.
        let num_sims = 5000;
        let count = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let count_clone = Rc::clone(&count);
            let mut context = Context::new();

            let parameters = match (current_status, next_status) {
                (SymptomStatus::NoSymptoms, SymptomStatus::Mild) => Params {
                    infect_to_mild: InfectToMildParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Severe) => Params {
                    mild_to_severe: MildToSevereParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Resolved) => Params {
                    mild_to_severe: MildToSevereParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    mild_to_resolved: MildToResolvedParameters {
                        mu: 0.1,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Critical) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Resolved) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    severe_to_resolved: SevereToResolvedParameters {
                        mu: 0.1,
                        sigma: 0.1,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Dead) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Resolved) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        mu: 0.1,
                        sigma: 0.1,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: expected_proportion,
                        }],
                    },
                    critical_to_resolved: CriticalToResolvedParameters {
                        mu: 0.1,
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
            // Infect the person to trigger the symptom status manager
            match next_status {
                SymptomStatus::NoSymptoms => (),
                SymptomStatus::Mild => context.infect_person(p1, None, None, None),
                _ => context.set_property::<Person, SymptomStatus>(p1, current_status),
            }
            // Add a plan to shutdown
            context.add_plan(100.0, ixa::Context::shutdown);

            context.subscribe_to_event(
                move |context, event: PropertyChangeEvent<Person, SymptomStatus>| match event
                    .current
                {
                    SymptomStatus::Mild => {
                        *count_clone.borrow_mut() += 1;
                        context.shutdown();
                    }
                    _ => context.shutdown(),
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
        // We start with 1 person and simulate infecting them 5000 times, we should see that the distribution of time
        // until they have mild symptoms is close to the expected distribution.
        let num_sims = 5000;
        let durations = Rc::new(RefCell::new(Vec::new()));
        for seed in 0..num_sims {
            let durations_clone = Rc::clone(&durations);
            let mut context = Context::new();

            let parameters = match (current_status, next_status) {
                (SymptomStatus::NoSymptoms, SymptomStatus::Mild) => Params {
                    infect_to_mild: InfectToMildParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 1.0,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Severe) => Params {
                    mild_to_severe: MildToSevereParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 1.0,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Mild, SymptomStatus::Resolved) => Params {
                    mild_to_severe: MildToSevereParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 0.0,
                        }],
                    },
                    mild_to_resolved: MildToResolvedParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Critical) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 1.0,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Severe, SymptomStatus::Resolved) => Params {
                    severe_to_critical: SevereToCriticalParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 0.0,
                        }],
                    },
                    severe_to_resolved: SevereToResolvedParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Dead) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 1.0,
                        }],
                    },
                    ..Default::default()
                },
                (SymptomStatus::Critical, SymptomStatus::Resolved) => Params {
                    critical_to_dead: CriticalToDeadParameters {
                        mu: expected_mu,
                        sigma: expected_sigma,
                        age_groups: vec![SymptomStatusAgeGroup {
                            min: 0,
                            probability: 0.0,
                        }],
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
            // Infect the person to trigger the symptom status manager
            match next_status {
                SymptomStatus::NoSymptoms => (),
                SymptomStatus::Mild => context.infect_person(p1, None, None, None),
                _ => context.set_property::<Person, SymptomStatus>(p1, current_status),
            }
            // Add a plan to shutdown after we see they progress to the next status
            context.add_plan(100.0, ixa::Context::shutdown);

            context.subscribe_to_event(
                move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                    if event.current == next_status {
                        durations_clone
                            .borrow_mut()
                            .push(context.get_current_time());
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
        println!(
            "Average duration from infection to mild symptoms: {}",
            average_duration
        );
        println!(
            "Expected duration from infection to mild symptoms: {}",
            expected_mu.exp()
        );
        assert_almost_eq!(average_duration, expected_mu.exp(), 0.1);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_proportion_infected_mild() {
        test_proportion(SymptomStatus::NoSymptoms, SymptomStatus::Mild, 0.3);
    }

    #[test]
    fn test_infection_to_mild_duration() {
        test_duration(SymptomStatus::NoSymptoms, SymptomStatus::Mild, 1.0, 0.1);
    }

    #[test]
    fn test_absorbing_states() {
        // We want to check that infected individuals eventually end up in an absorbing state (No Syptoms, Resolved, or Dead).
        let num_sims: u64 = 1000;
        let mut count_no_symptoms: u64 = 0;
        let mut count_resolved: u64 = 0;
        let mut count_dead: u64 = 0;
        for seed in 0..num_sims {
            let mut context = Context::new();
            let parameters = Params {
                infect_to_mild: InfectToMildParameters {
                    mu: 0.1,
                    sigma: 0.1,
                    age_groups: vec![SymptomStatusAgeGroup {
                        min: 0,
                        probability: 0.5,
                    }],
                },
                mild_to_severe: MildToSevereParameters {
                    mu: 0.1,
                    sigma: 0.1,
                    age_groups: vec![SymptomStatusAgeGroup {
                        min: 0,
                        probability: 0.5,
                    }],
                },
                mild_to_resolved: MildToResolvedParameters {
                    mu: 0.1,
                    sigma: 0.1,
                },
                severe_to_critical: SevereToCriticalParameters {
                    mu: 0.1,
                    sigma: 0.1,
                    age_groups: vec![SymptomStatusAgeGroup {
                        min: 0,
                        probability: 0.5,
                    }],
                },
                severe_to_resolved: SevereToResolvedParameters {
                    mu: 0.1,
                    sigma: 0.1,
                },
                critical_to_dead: CriticalToDeadParameters {
                    mu: 0.1,
                    sigma: 0.1,
                    age_groups: vec![SymptomStatusAgeGroup {
                        min: 0,
                        probability: 0.5,
                    }],
                },
                critical_to_resolved: CriticalToResolvedParameters {
                    mu: 0.1,
                    sigma: 0.1,
                },
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
        println!("Count No Symptoms: {}", count_no_symptoms);
        println!("Count Resolved: {}", count_resolved);
        println!("Count Dead: {}", count_dead);
        assert_eq!(count_no_symptoms + count_resolved + count_dead, num_sims);
    }
}

// All persons should end up with a SymptomStatus of NoSymptoms, Dead, or Resolved.

// TODO:
// -set up parameters (essential to make it run) [DONE]
// -add SymptomStatusData or some other mechanism to track what most symptom statuses a person had, even after resolved (and maybe track timing, too)
// -make probabilities age (category) specific
// -finish validation for parameters
