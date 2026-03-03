use ixa::{Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_rng, impl_property, prelude::PropertyChangeEvent};
use rand_distr::{LogNormal};
use serde::{Deserialize, Serialize};

use crate::{
    ContextParametersExt, Params, infectiousness_manager::InfectionStatus, population_loader::Person
};

define_rng!(SymptomsRng);

#[derive(Serialize, Deserialize, PartialEq, Debug, Copy, Clone, Eq, Hash)]
pub enum SymptomStatus {
    NoSymptoms,
    Mild,
    Severe,
    Critical,
    Resolved,
    Dead
}

impl_property!(
    SymptomStatus,
    Person,
    default_const = SymptomStatus::NoSymptoms
);

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params{
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
        move|context, event: PropertyChangeEvent<Person, InfectionStatus>|{
            if event.current == InfectionStatus::Infectious {
                if context.sample_bool(SymptomsRng, probability_mild_given_infect){
                    let infect_to_mild = LogNormal::new(infect_to_mild_mu, infect_to_mild_sigma).unwrap();
                    let mild_time = context.get_current_time() + context.sample_distr(SymptomsRng,   infect_to_mild);
                    context.add_plan(mild_time, move|context|{
                        context.set_property(event.entity_id,SymptomStatus::Mild);
                    });
                }
            }
        }
    );

    context.subscribe_to_event(
        move|context, event: PropertyChangeEvent<Person, SymptomStatus>|{
            match event.current {
                SymptomStatus::Mild => {
                    if context.sample_bool(SymptomsRng, probability_severe_given_mild){
                        let mild_to_severe = LogNormal::new(mild_to_severe_mu, mild_to_severe_sigma).unwrap();
                        let severe_time = context.get_current_time() + context.sample_distr(SymptomsRng, mild_to_severe);
                        context.add_plan(severe_time, move|context|{
                            context.set_property(event.entity_id, SymptomStatus::Severe);
                        });
                    } else {
                        let mild_to_resolved = LogNormal::new(mild_to_resolved_mu, mild_to_resolved_sigma).unwrap();
                        let resolved_time = context.get_current_time() + context.sample_distr(SymptomsRng, mild_to_resolved);
                        context.add_plan(resolved_time, move|context|{
                            context.set_property(event.entity_id, SymptomStatus::Resolved);
                        });
                    }
                },
                SymptomStatus::Severe => {
                    if context.sample_bool(SymptomsRng, probability_critical_given_severe){
                        let severe_to_critical = LogNormal::new(severe_to_critical_mu, severe_to_critical_sigma).unwrap();
                        let critical_time = context.get_current_time() + context.sample_distr(SymptomsRng, severe_to_critical);
                        context.add_plan(critical_time, move|context|{
                            context.set_property(event.entity_id, SymptomStatus::Critical);
                        });
                    } else {
                        let severe_to_resolved = LogNormal::new(severe_to_resolved_mu, severe_to_resolved_sigma).unwrap();
                        let resolved_time = context.get_current_time() + context.sample_distr(SymptomsRng, severe_to_resolved);
                        context.add_plan(resolved_time, move|context|{
                            context.set_property(event.entity_id, SymptomStatus::Resolved);
                        });
                    }
                },
                SymptomStatus::Critical => {
                    if context.sample_bool(SymptomsRng, probability_dead_given_critical){
                        let critical_to_dead = LogNormal::new(critical_to_dead_mu, critical_to_dead_sigma).unwrap();
                        let dead_time = context.get_current_time() + context.sample_distr(SymptomsRng, critical_to_dead);
                        context.add_plan(dead_time, move|context|{
                            context.set_property(event.entity_id, SymptomStatus::Dead);
                        });
                    } else {
                        let critical_to_resolved = LogNormal::new(critical_to_resolved_mu, critical_to_resolved_sigma).unwrap();
                        let resolved_time = context.get_current_time() + context.sample_distr(SymptomsRng, critical_to_resolved);
                        context.add_plan(resolved_time, move|context|{
                            context.set_property(event.entity_id, SymptomStatus::Resolved);
                        });
                    }
                },
                _ => ()
            }
        });
    Ok(())
}

// All persons should end up with a SymptomStatus of NoSymptoms, Dead, or Resolved.

// TODO:
// -set up parameters (essential to make it run) [DONE]
// -add SymptomStatusData or some other mechanism to track what most symptom statuses a person had, even after resolved (and maybe track timing, too)
// -make probabilities age (category) specific
// -finish validation for parameters