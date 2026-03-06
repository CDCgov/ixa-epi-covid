use ixa::{Context, HashMap, IxaError, plan::PlanId, prelude::*};

use crate::{
    population_loader::{Alive, Person, PersonId},
    settings::ContextSettingExt,
    symptom_status_manager::SymptomStatus,
};

#[derive(Default)]
struct DeathDataContainer {
    plans: HashMap<PersonId, Vec<PlanId>>,
}

impl DeathDataContainer {
    fn record_plan(&mut self, person: PersonId, plan: PlanId) {
        self.plans.entry(person).or_default().push(plan);
    }

    fn get_plans(&mut self, person: PersonId) -> Vec<PlanId> {
        self.plans.get(&person).cloned().unwrap_or_default()
    }

    fn remove_plan_records(&mut self, person: PersonId) {
        self.plans.remove(&person);
    }
}

define_data_plugin!(
    DeathDataPlugin,
    DeathDataContainer,
    DeathDataContainer::default()
);

pub trait ContextDeathExt: PluginContext + ContextEntitiesExt {
    fn is_alive(&self, person_id: PersonId) -> bool {
        self.get_property::<Person, Alive>(person_id).0
    }
    fn record_plan(&mut self, person_id: PersonId, plan_id: PlanId) {
        self.get_data_mut(DeathDataPlugin)
            .record_plan(person_id, plan_id);
    }

    fn cancel_plans(&mut self, person_id: PersonId) {
        let plan_ids = self.get_data_mut(DeathDataPlugin).get_plans(person_id);
        for plan_id in plan_ids {
            self.cancel_plan(&plan_id);
        }
        self.get_data_mut(DeathDataPlugin)
            .remove_plan_records(person_id);
    }
}
impl ContextDeathExt for Context {}
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.subscribe_to_event(
        move |context, event: PropertyChangeEvent<Person, SymptomStatus>| {
            if event.current == SymptomStatus::Dead {
                context.set_property::<Person, Alive>(event.entity_id, Alive(false));
                context.remove_person_from_settings(event.entity_id);
                context.cancel_plans(event.entity_id);
            }
        },
    );
    Ok(())
}

#[cfg(test)]
mod test {
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
        // We schedule the person to be set to dead at time 0.0,
        // and then we check their alive status both before and after the death event is processed.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(context.is_alive(person_id));
                context.set_property::<Person, SymptomStatus>(person_id, SymptomStatus::Dead);
            },
            ExecutionPhase::First,
        );
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(!context.is_alive(person_id));
            },
            ExecutionPhase::Last,
        );
        context.execute();
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
        // We schedule the person to be set to dead at time 0.0,
        // and then we check that they are removed from the homogeneous mixing setting after the death event is processed.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(context.is_alive(person_id));
                assert!(
                    context
                        .get_setting_members(&SettingId::new(HomogeneousMixing, 0))
                        .unwrap()
                        .contains(&person_id)
                );
                context.set_property::<Person, SymptomStatus>(person_id, SymptomStatus::Dead);
            },
            ExecutionPhase::First,
        );
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(!context.is_alive(person_id));
                assert!(
                    !context
                        .get_setting_members(&SettingId::new(HomogeneousMixing, 0))
                        .unwrap()
                        .contains(&person_id)
                );
            },
            ExecutionPhase::Last,
        );
        context.execute();
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
                    if event.current == InfectionStatus::Infectious {
                        if event.entity_id != infectious_person {
                            *num_infected_clone.borrow_mut() += 1;
                            context.set_property(event.entity_id, InfectionData::Susceptible);
                        }
                    }
                },
            );
            context.execute();
        }
        // assert the susceptible person was never infected across all simulations
        assert_eq!(*num_infected.borrow(), 0);
    }
}
