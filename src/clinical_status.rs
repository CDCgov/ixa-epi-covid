use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, IxaError, define_rng, impl_derived_property, impl_property, prelude::PropertyChangeEvent
};
use rand_distr::Exp;
use serde::{Deserialize, Serialize};

use crate::{
    Params, infectiousness_manager::InfectionStatus, parameters::ContextParametersExt, population_loader::Person,
};

#[derive(Serialize, PartialEq, Debug, Clone, Copy)]
pub enum SymptomData {
    Asymptomatic,
    Symptomatic {
        symptom_onset: f64,
    },    
    Recovered {
        symptom_onset: f64,
        recovery_time: f64,
    },
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Eq, Hash)]
pub enum SymptomStatus {
    Asymptomatic,
    Symptomatic,
    Recovered,
}

impl_property!(
    SymptomData,
    Person,
    default_const = SymptomData::Asymptomatic
);

impl_derived_property!(
    SymptomStatus,
    Person,
    [SymptomData], [],
    |data| match data {
        SymptomData::Asymptomatic => SymptomStatus::Asymptomatic,
        SymptomData::Symptomatic { .. } => SymptomStatus::Symptomatic,
        SymptomData::Recovered { .. } => SymptomStatus::Recovered,
    }
);

define_rng!(SymptomRng);

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        probability_symptoms,
        incubation_period,
        symptom_duration,
        ..
    } = context.get_params();
    // Subscribe to a person being infected to determine whether they will develop symptoms or not
    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current == InfectionStatus::Infectious
                && context.sample_bool(SymptomRng, probability_symptoms) {
                    let exp = Exp::new(incubation_period).unwrap();
                    let symptom_onset = context.get_current_time() + context.sample_distr(SymptomRng, exp);
                    context.add_plan(symptom_onset, move |context| {
                        context.set_property(event.entity_id, SymptomData::Symptomatic {symptom_onset: context.get_current_time()}
                        );
                    }
                    );
                }
        }
    );

    // Subscribe to a person being symptomatic and scheduling recovery
    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
            if event.current == SymptomStatus::Symptomatic {
                let exp = Exp::new(symptom_duration).unwrap();
                let recovery_time = context.get_current_time() + context.sample_distr(SymptomRng, exp);
                context.add_plan(recovery_time, move |context| {
                    let recovery_time= context.get_current_time();
                    let SymptomData::Symptomatic { symptom_onset } = context.get_property::<Person, SymptomData>(event.entity_id)
                        else {
                            panic!("Can't recover from symptoms if not symptomatic");
                        };
                    context.set_property(event.entity_id, SymptomData::Recovered {
                        symptom_onset,
                        recovery_time,
                    }
                        );
                    }
                    );
                }
        }
    );
    Ok(())
}
