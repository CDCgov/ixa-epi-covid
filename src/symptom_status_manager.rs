use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_rng, impl_property,
    prelude::PropertyChangeEvent,
};
use rand_distr::LogNormal;
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params, infectiousness_manager::InfectionStatus,
    population_loader::Person,
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

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        probability_mild_given_infect,
        infect_to_mild_mu,
        infect_to_mild_sigma,
        probability_severe_given_mild,
        mild_to_severe_mu,
        mild_to_severe_sigma,
        mild_to_resolved_mu,
        mild_to_resolved_sigma,
        probability_critical_given_severe,
        severe_to_critical_mu,
        severe_to_critical_sigma,
        severe_to_resolved_mu,
        severe_to_resolved_sigma,
        probability_dead_given_critical,
        critical_to_dead_mu,
        critical_to_dead_sigma,
        critical_to_resolved_mu,
        critical_to_resolved_sigma,
        ..
    } = context.get_params();

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current == InfectionStatus::Infectious {
                if context.sample_bool(SymptomsRng, probability_mild_given_infect) {
                    let infect_to_mild =
                        LogNormal::new(infect_to_mild_mu, infect_to_mild_sigma).unwrap();
                    let mild_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, infect_to_mild);
                    context.add_plan(mild_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Mild);
                    });
                }
            }
        },
    );

    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| match event.current {
            SymptomStatus::Mild => {
                if context.sample_bool(SymptomsRng, probability_severe_given_mild) {
                    let mild_to_severe =
                        LogNormal::new(mild_to_severe_mu, mild_to_severe_sigma).unwrap();
                    let severe_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, mild_to_severe);
                    context.add_plan(severe_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Severe);
                    });
                } else {
                    let mild_to_resolved =
                        LogNormal::new(mild_to_resolved_mu, mild_to_resolved_sigma).unwrap();
                    let resolved_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, mild_to_resolved);
                    context.add_plan(resolved_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Resolved);
                    });
                }
            }
            SymptomStatus::Severe => {
                if context.sample_bool(SymptomsRng, probability_critical_given_severe) {
                    let severe_to_critical =
                        LogNormal::new(severe_to_critical_mu, severe_to_critical_sigma).unwrap();
                    let critical_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, severe_to_critical);
                    context.add_plan(critical_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Critical);
                    });
                } else {
                    let severe_to_resolved =
                        LogNormal::new(severe_to_resolved_mu, severe_to_resolved_sigma).unwrap();
                    let resolved_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, severe_to_resolved);
                    context.add_plan(resolved_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Resolved);
                    });
                }
            }
            SymptomStatus::Critical => {
                if context.sample_bool(SymptomsRng, probability_dead_given_critical) {
                    let critical_to_dead =
                        LogNormal::new(critical_to_dead_mu, critical_to_dead_sigma).unwrap();
                    let dead_time = context.get_current_time()
                        + context.sample_distr(SymptomsRng, critical_to_dead);
                    context.add_plan(dead_time, move |context| {
                        context.set_property(event.entity_id, SymptomStatus::Dead);
                    });
                } else {
                    let critical_to_resolved =
                        LogNormal::new(critical_to_resolved_mu, critical_to_resolved_sigma)
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
    use crate::parameters::GlobalParams;
    use crate::population_loader::Person;
    use crate::symptom_status_manager::SymptomStatus;
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

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_proportion_infected_mild() {
        // We start with 1 person and simulate infecting them 5000 times, we should see that the proportion of time
        // that they have mild symptoms is close to the expected probability of mild symptoms given infection.
        // We use a large number of simulations to ensure that we have enough mild cases to analyze, since the outcome is stochastic.
        let num_sims = 5000;
        let probability_mild_given_infect = 0.5;

        let num_people_mild = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let num_people_mild_clone = Rc::clone(&num_people_mild);
            let mut context = Context::new();

            let parameters = Params {
                probability_mild_given_infect,
                ..Default::default()
            };

            context.init_random(parameters.seed);
            context
                .set_global_property_value(GlobalParams, parameters)
                .unwrap();

            context.init_random(seed);

            // Add our person
            let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
            // Initialize event subscriptions and plans for symptom status manager
            init(&mut context).unwrap();
            // Infect the person to trigger the symptom status manager
            context.infect_person(p1, None, None, None);
            // Add a plan to shutdown
            context.add_plan(100.0, ixa::Context::shutdown);

            context.subscribe_to_event(
                move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                    if event.current == SymptomStatus::Mild {
                        *num_people_mild_clone.borrow_mut() += 1;
                        context.shutdown();
                    }
                },
            );
            // Run the simulation
            context.execute();
        }
        // Check that the proportion of people is close to the expected proportion
        assert_almost_eq!(
            *num_people_mild.borrow() as f64 / (num_sims) as f64,
            probability_mild_given_infect,
            0.01
        );
    }

    #[test]
    fn test_infection_to_mild_duration() {
        // We start with 1 person and simulate infecting them 5000 times,
        // we should see that the distribution of time until they have mild symptoms is close to the expected distribution.
        let num_sims = 5000;
        let probability_mild_given_infect = 1.0; // Set to 1 to ensure we have enough mild cases to analyze
        let infect_to_mild_mu = 1.0;
        let infect_to_mild_sigma = 0.1; // As sigma increases the more like the test will fail

        let durations = Rc::new(RefCell::new(Vec::new()));
        for seed in 0..num_sims {
            let durations_clone = Rc::clone(&durations);
            let mut context = Context::new();

            let parameters = Params {
                probability_mild_given_infect,
                infect_to_mild_mu,
                infect_to_mild_sigma,
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
            // Add a plan to shutdown after we see they progress to mild symptoms
            context.add_plan(100.0, ixa::Context::shutdown);

            // Subscribe to symptom status changes to record the time until the person has mild symptoms
            // Shutdown after we see they progress to mild symptoms
            context.subscribe_to_event(
                move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                    if event.current == SymptomStatus::Mild {
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
            infect_to_mild_mu.exp()
        );
        assert_almost_eq!(average_duration, infect_to_mild_mu.exp(), 0.1);
    }
}

// All persons should end up with a SymptomStatus of NoSymptoms, Dead, or Resolved.

// TODO:
// -set up parameters (essential to make it run) [DONE]
// -add SymptomStatusData or some other mechanism to track what most symptom statuses a person had, even after resolved (and maybe track timing, too)
// -make probabilities age (category) specific
// -finish validation for parameters
