use ixa::{Context, IxaError, plan::PlanId, prelude::*};

use crate::{
    infectiousness_manager::{InfectionContextExt, InfectionStatus},
    population_loader::{Alive, Person, PersonId},
    settings::ContextSettingExt,
    symptom_status_manager::SymptomStatus,
};

pub trait ContextDeathExt: PluginContext + ContextEntitiesExt {
    fn is_alive(&self, person_id: PersonId) -> bool {
        self.get_property::<Person, Alive>(person_id).0
    }

    /// Adds a plan for the given person if and only if that person is
    /// alive when the plan comes due.
    fn add_plan_if_alive(
        &mut self,
        person_id: PersonId,
        time: f64,
        callback: impl FnOnce(&mut Context) + 'static,
    ) -> PlanId {
        self.add_plan(time, move |context| {
            // Only execute callback if the person is still alive.
            if context.is_alive(person_id) {
                callback(context)
            }
        })
    }
}
impl ContextDeathExt for Context {}
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
            if event.current == SymptomStatus::Dead {
                // When the person's symptom status changes to Dead, we set their Alive property to false and remove them from all settings.
                // If the person is currently infectious, we also immediately transition them to recovered.
                context.set_property::<Person, Alive>(event.entity_id, Alive(false));
                context.remove_person_from_settings(event.entity_id);
                if context.get_property::<Person, InfectionStatus>(event.entity_id)
                    == InfectionStatus::Infectious
                {
                    context.recover_person(event.entity_id);
                }
            }
        },
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use core::panic;
    use std::{cell::RefCell, rc::Rc};

    use ixa::{ExecutionPhase, prelude::*};

    use ixa::HashMap;

    use super::init;
    use crate::death::ContextDeathExt;
    use crate::infectiousness_manager::{InfectionData, InfectionStatus};
    use crate::population_loader::PersonId;
    use crate::symptom_status_manager::{self, SymptomStatus};
    use crate::{Age, infection_propagation_loop};
    use crate::{
        define_setting_category,
        infectiousness_manager::InfectionContextExt,
        parameters::{CoreSettingsTypes, GlobalParams, Params, RateFnType},
        population_loader::Person,
        settings::{ContextSettingExt, ItineraryEntry, SettingId, SettingProperties},
    };

    define_setting_category!(HomogeneousMixing);

    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = vec![ItineraryEntry::new(
            SettingId::new(HomogeneousMixing, 0),
            1.0,
        )];
        context.add_itinerary(person_id, itinerary)
    }

    fn setup_context(seed: u64, rate: f64, alpha: f64, duration: f64) -> Context {
        let mut context = Context::new();

        let parameters = Params {
            seed,
            max_time: 100.0,
            infectiousness_rate_fn: RateFnType::Constant { rate, duration },
            settings_properties: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, SettingProperties { alpha: 0.5 }),
                    (
                        CoreSettingsTypes::Workplace,
                        SettingProperties { alpha: 0.5 },
                    ),
                    (
                        CoreSettingsTypes::CensusTract,
                        SettingProperties {
                            alpha: 0.5,
                            // Itinerary is specified in the `set_homogeneous_mixing_itinerary` function
                            // so we do not need to set it here.
                        },
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (CoreSettingsTypes::Home, 1.0),
                (CoreSettingsTypes::Workplace, 1.0),
                (CoreSettingsTypes::CensusTract, 0.0),
            ]),
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        // We also set up a homogenous mixing itinerary so that when we don't call `settings::init`,
        // we still have people in settings.
        context
            .register_setting_category(&HomogeneousMixing, SettingProperties { alpha }, 1.0)
            .unwrap();
        context
    }

    #[test]
    fn test_alive_property_set_to_false_on_death() {
        // This test verifies that when a person's symptom status is set to Dead, their Alive property is set to false.
        // It uses the ContextDeathExt trait to check the alive status of the person before and after setting them to dead.
        let mut context = setup_context(0, 5.0, 0.5, 10.0);
        let person_id: PersonId = context.add_entity((Age(30),)).unwrap();
        symptom_status_manager::init(&mut context).unwrap();
        init(&mut context).unwrap();
        // We set the person to be dead,
        // and then we check their alive status both before and after the death event is processed.
        context.set_property::<Person, SymptomStatus>(person_id, SymptomStatus::Dead);
        context.execute();
        assert!(!context.is_alive(person_id));
    }

    #[test]
    fn test_setting_removal_on_death() {
        // This test verifies that when a person dies, they are removed from all settings they were a part of.
        // We create a person, add them to a homogeneous mixing setting, and then set them
        // to dead. We check that they are removed from the setting after death.
        let mut context = setup_context(0, 5.0, 0.5, 10.0);
        let person_id: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, person_id).unwrap();
        symptom_status_manager::init(&mut context).unwrap();
        init(&mut context).unwrap();
        // We set the person to be dead
        // and then we check that they are removed from the homogeneous mixing setting after the death event is processed.
        assert!(context.is_alive(person_id));
        assert!(
            context
                .get_setting_members(&SettingId::new(HomogeneousMixing, 0))
                .unwrap()
                .contains(&person_id)
        );
        context.set_property::<Person, SymptomStatus>(person_id, SymptomStatus::Dead);
        context.execute();
        assert!(!context.is_alive(person_id));
        assert!(
            !context
                .get_setting_members(&SettingId::new(HomogeneousMixing, 0))
                .unwrap()
                .contains(&person_id)
        );
    }

    #[test]
    fn test_dead_person_recovers_immediately() {
        // This test verifies that when a person is dies while infectious, they immediately transition to a recovered state.
        // This test also verifies that the plan to recovered created from the infection event does not execute.
        // We create a person, infect them, set them to dead, and then check they are recovered and that they do not recover twice.
        let mut context = setup_context(0, 5.0, 0.5, 10.0);
        let person_id: PersonId = context.add_entity((Age(30),)).unwrap();
        infection_propagation_loop::init(&mut context).unwrap();
        symptom_status_manager::init(&mut context).unwrap();
        init(&mut context).unwrap();
        // We set the person to be infectious and then immediately set them to dead
        // and then we check that their infection status changes to recovered after the death event is processed.
        assert!(context.is_alive(person_id));
        context.infect_person(person_id, None, None, None);
        context.set_property::<Person, SymptomStatus>(person_id, SymptomStatus::Dead);
        context.execute();
        match context.get_property::<Person, InfectionData>(person_id) {
            InfectionData::Recovered {
                infection_time,
                recovery_time,
            } => {
                assert_eq!(infection_time, recovery_time);
            }
            InfectionData::Susceptible => {
                panic!("Person should not be susceptible after being infected and then dying")
            }
            InfectionData::Infectious { .. } => {
                panic!("Person should not be infectious after being infected and then dying")
            }
        }
    }

    #[test]
    fn test_no_infection_when_dead() {
        // This test verifies that when a person is dead, they cannot infect others.
        // We create an infectious person and a susceptible person, set the infectious person to dead,
        // and then check that the susceptible person does not become infected over the course of the simulation
        let num_sims: u64 = 1000;
        let rate = 5.0;
        let alpha = 0.42;
        let duration = 10.0;

        let num_infected = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let num_infected_clone = Rc::clone(&num_infected);
            let mut context = setup_context(seed, rate, alpha, duration);

            context.add_plan_with_phase(10.0, ixa::Context::shutdown, ExecutionPhase::Last);
            // Add our susceptible fellow and set their itinerary.
            let p1: PersonId = context.add_entity((Age(30),)).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();

            // Add our infectious fellow and set their itinerary
            let infectious_person: PersonId = context.add_entity((Age(30),)).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, infectious_person).unwrap();

            // Initialize the infection propagation loop, symptom status manager, and death modules,
            // and then infect the infectious person and set them to dead.
            infection_propagation_loop::init(&mut context).unwrap();
            symptom_status_manager::init(&mut context).unwrap();
            init(&mut context).unwrap();
            context.infect_person(infectious_person, None, None, None);
            context.set_property::<Person, SymptomStatus>(infectious_person, SymptomStatus::Dead);

            // Add a watcher if the other person is infected.
            context.subscribe_to_event::<PropertyChangeEvent<Person, InfectionStatus>>(
                move |context, event| {
                    if event.current == InfectionStatus::Infectious
                        && event.entity_id != infectious_person
                    {
                        *num_infected_clone.borrow_mut() += 1;
                        context.set_property(event.entity_id, InfectionData::Susceptible);
                    }
                },
            );
            context.execute();
        }
        // assert the susceptible person was never infected across all simulations
        assert_eq!(*num_infected.borrow(), 0);
    }
}
