use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_rng, impl_property,
    prelude::PropertyChangeEvent,
};
use rand_distr::LogNormal;
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params, error::ModelError, infectiousness_manager::InfectionStatus, population_loader::{Person, PersonId}
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

    use super::init;
    use crate::infectiousness_manager::InfectionContextExt;
    use crate::parameters::GlobalParams;
    use crate::population_loader::Person;
    use crate::symptom_status_manager::{
        SymptomDelayDistLogNormParams, SymptomStatus, plan_symptom_transition,
        process_symptom_change_event,
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
    fn test_plan_symptom_transition() {
        let mut context = Context::new();
        let log_normal_delay_params = SymptomDelayDistLogNormParams {
            mu: 1.0,
            sigma: 0.0,
        };
        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.init_random(1234);
        assert_eq!(
            context.get_property::<Person, SymptomStatus>(p1),
            SymptomStatus::NoSymptoms // this helps check defaults
        );
        plan_symptom_transition(
            &mut context,
            p1,
            SymptomStatus::Mild,
            log_normal_delay_params,
        );
        context.execute();
        assert_eq!(
            context.get_property::<Person, SymptomStatus>(p1),
            SymptomStatus::Mild
        );
        assert_almost_eq!(
            context.get_current_time(),
            log_normal_delay_params.mu.exp(),
            1e-8
        );
    }

    // test process_symptom_change_event by specifying a given symptom status and probability
    // and then checking that the person moves to the correct symptom status
    // because the function has "hard coded" transitions, we need multiple tests
    #[test]
    fn test_mild_to_severe_symptom_change_event() {
        let mut context = Context::new();
        let parameters = Params {
            probability_severe_given_mild: 1.0,
            mild_to_severe_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                process_symptom_change_event(context, event);
            },
        );
        context.set_property::<Person, SymptomStatus>(p1, SymptomStatus::Mild);
        context.add_plan_with_phase(
            parameters.mild_to_severe_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Severe
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
    }

    #[test]
    fn test_mild_to_resolved_symptom_change_event() {
        let mut context = Context::new();
        let parameters = Params {
            probability_severe_given_mild: 0.0,
            mild_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                process_symptom_change_event(context, event);
            },
        );
        context.set_property::<Person, SymptomStatus>(p1, SymptomStatus::Mild);
        context.add_plan_with_phase(
            parameters.mild_to_resolved_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Resolved
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
    }

    #[test]
    fn test_severe_to_critical_symptom_change_event() {
        let mut context = Context::new();
        let parameters = Params {
            probability_critical_given_severe: 1.0,
            severe_to_critical_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                process_symptom_change_event(context, event);
            },
        );
        context.set_property::<Person, SymptomStatus>(p1, SymptomStatus::Severe);
        context.add_plan_with_phase(
            parameters.severe_to_critical_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Critical
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
    }

    #[test]
    fn test_severe_to_resolved_symptom_change_event() {
        let mut context = Context::new();
        let parameters = Params {
            probability_critical_given_severe: 0.0,
            severe_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                process_symptom_change_event(context, event);
            },
        );
        context.set_property::<Person, SymptomStatus>(p1, SymptomStatus::Severe);
        context.add_plan_with_phase(
            parameters.severe_to_resolved_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Resolved
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
    }

    #[test]
    fn test_critical_to_dead_symptom_change_event() {
        let mut context = Context::new();
        let parameters = Params {
            probability_dead_given_critical: 1.0,
            critical_to_dead_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                process_symptom_change_event(context, event);
            },
        );
        context.set_property::<Person, SymptomStatus>(p1, SymptomStatus::Critical);
        context.add_plan_with_phase(
            parameters.severe_to_critical_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Dead
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
    }

    #[test]
    fn test_critical_to_resolved_symptom_change_event() {
        let mut context = Context::new();
        let parameters = Params {
            probability_critical_given_severe: 0.0,
            critical_to_resolved_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
                process_symptom_change_event(context, event);
            },
        );
        context.set_property::<Person, SymptomStatus>(p1, SymptomStatus::Critical);
        context.add_plan_with_phase(
            parameters.critical_to_resolved_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Resolved
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
    }

    // testing NoSymptoms to Mild is a bit different because the trigger is an infection
    // and this is coded in the init, rather than using process_symptom_change_event
    #[test]
    fn test_no_symptoms_to_mild() {
        let mut context = Context::new();
        let parameters = Params {
            probability_mild_given_infect: 1.0,
            infect_to_mild_delay: SymptomDelayDistLogNormParams {
                mu: 0.0,
                sigma: 0.0,
            },
            ..Default::default()
        };
        context.init_random(123);
        context
            .set_global_property_value(GlobalParams, parameters.clone())
            .unwrap();

        let p1 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        init(&mut context).unwrap();
        context.infect_person(p1, None, None, None);
        context.add_plan_with_phase(
            parameters.infect_to_mild_delay.mu.exp(),
            move |context| {
                assert_eq!(
                    context.get_property::<Person, SymptomStatus>(p1),
                    SymptomStatus::Mild
                );
                context.shutdown();
            },
            ixa::ExecutionPhase::Last,
        );
        context.execute();
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
        context.infect_person(p1, None, None, None);
        context.execute();
        assert_eq!(
            context.get_property::<Person, SymptomStatus>(p1),
            SymptomStatus::NoSymptoms
        );
    }

    #[test]
    fn test_absorbing_states() {
        // We want to check that infected individuals eventually end up in an absorbing state (No Syptoms, Resolved, or Dead).
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
