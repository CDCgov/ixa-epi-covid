use ixa::prelude::*;
use std::{any::TypeId, collections::HashMap};

use crate::{
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    settings::{Person, PersonId},
};

/// Defines a transmission modifier that is used to modify the transmissiveness or susceptibility
/// of a person based on their infection status.
// We require `Debug` for easy logging of the trait so the user can see what is happening.
pub trait TransmissionModifier: std::fmt::Debug + 'static {
    /// Return the relative potential for infection (transmissiveness or susceptibility) for a person
    /// based on their infection status.
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64;
}

// A type alias for the type of the transmission modifiers specified via a hashmap of person
// property values and floats -- i.e., `modifier_key: &[(P::Value, f64)]`
#[derive(Debug)]
struct PersonPropertyTransmissionModifier<P> {
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    transmission_modifier_map: HashMap<P, f64>,
}

impl<P> TransmissionModifier for PersonPropertyTransmissionModifier<P>
where
    P: IndexableProperty<Person>,
{
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64 {
        let property_val = context.get_property::<Person, P>(person_id);
        // Return the corresponding value from the map, or 1.0 if not found
        *self
            .transmission_modifier_map
            .get(&property_val)
            .unwrap_or(&1.0)
    }
}

#[derive(Default)]
struct TransmissionModifierContainer {
    transmission_modifier_map:
        HashMap<InfectionStatus, HashMap<TypeId, Box<dyn TransmissionModifier>>>,
}

define_data_plugin!(
    TransmissionModifierPlugin,
    TransmissionModifierContainer,
    TransmissionModifierContainer::default()
);

pub trait ContextTransmissionModifierExt: PluginContext {
    /// Register a generic transmission modifier for a specific infection status.
    fn register_transmission_modifier_fn<T: TransmissionModifier>(
        &mut self,
        infection_status: InfectionStatus,
        transmission_modifier: T,
    ) {
        // Box the transmission modifier to store it in the map
        // Transmission modifiers must implement debug so that we can more easily log their addition
        let boxed_transmission_modifier = Box::new(transmission_modifier);

        // Insert the boxed function into the transmission modifier map, using entry to handle unititialized keys
        if let Some(modifier_fxn) = self
            .get_data_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), boxed_transmission_modifier)
        {
            trace!(
                "Overwriting existing transmission modifier function for infection status {infection_status:?} and transmission modifier {:?}",
                modifier_fxn
            );
        }
    }

    /// Register a transmission modifier that depends solely on the value of one person property.
    /// The function accepts a relative transmission potential key, which is a slice of tuples that
    /// associate values of a specified person property with the relativ etransmission potential of
    /// a person with that property value. All floats declared in this fashion must be between zero
    /// and one and represent the proportion of infectiousness or susceptiblity remaining if a
    /// modifier is active.
    ///
    /// Any modifiers based on efficacy (e.g. facemask transmission prevention) should be
    /// subtracted from 1.0 for effect on relative transmission potential.
    ///
    /// Internally, this method registers a transmission modifier function that returns the float
    /// value associated the person's property value in the
    /// `relative_transmission_potential_multipliers` key.
    #[allow(dead_code)]
    fn store_transmission_modifier_values<P>(
        &mut self,
        infection_status: InfectionStatus,
        relative_transmission_potential_multipliers: &[(P, f64)],
    ) -> Result<(), ModelError>
    where
        P: IndexableProperty<Person>,
    {
        // Convert modifiers to HashMap
        let mut modifier_map = HashMap::new();
        for &(key, value) in relative_transmission_potential_multipliers.iter() {
            if !(0.0..=1.0).contains(&value) {
                return Err(ModelError::ModelError(
                    "Scalar modifier values stored must be between 0.0 and 1.0. ".to_string()
                        + &format!("Value {value} for {key:?} is not."),
                ));
            }

            if let Some(old_value) = modifier_map.insert(key, value) {
                return Err(ModelError::ModelError(
                    "Duplicate values provided in modifier key ".to_string()
                        + &format!(
                            "Values {old_value} and {value} were both attempted to be registered to key {key:?}"
                        ),
                ));
            }
        }

        // Register a default function to simply map floats with T::Values
        self.register_transmission_modifier_fn(
            infection_status,
            PersonPropertyTransmissionModifier {
                transmission_modifier_map: modifier_map,
            },
        );
        Ok(())
    }

    /// Get the relative potential for infection (infectiousness or susceptibility) for a person
    /// based on their infection status based on all registered modifiers. Queries all registered
    /// modifier functions and evaluates them based on the person's properties. Multiplies them
    /// together to get the total relative transmission modifier for the person.
    /// Returns 1.0 if no modifiers are registered for the person's infection status.
    fn get_relative_total_transmission(&self, person_id: PersonId) -> f64;
}

impl ContextTransmissionModifierExt for Context {
    fn get_relative_total_transmission(&self, person_id: PersonId) -> f64 {
        let infection_status = self.get_property::<Person, InfectionStatus>(person_id);

        let transmission_modifier_plugin = self.get_data(TransmissionModifierPlugin);
        if let Some(transmission_modifier_map) = transmission_modifier_plugin
            .transmission_modifier_map
            .get(&infection_status)
        {
            // Calculate the relative modifier for each registered function and multiply them
            // together to get the total relative transmission modifier for the person
            transmission_modifier_map
                .iter()
                .fold(1.0, |agg, (_type_id, transmission_modifier)| {
                    agg * transmission_modifier.get_relative_transmission(self, person_id)
                })
        } else {
            // If the infection status is not found in the map, return 1.0
            1.0
        }
    }
}

#[cfg(test)]
mod test {
    use ixa::assert_almost_eq;
    use ixa::prelude::*;
    use serde::{Deserialize, Serialize};

    use super::{
        ContextTransmissionModifierExt, PersonPropertyTransmissionModifier, TransmissionModifier,
        TransmissionModifierPlugin,
    };
    use crate::Age;
    use crate::error::ModelError;
    use crate::settings::{Person, PersonId};
    use crate::{
        infectiousness_manager::{InfectionContextExt, InfectionStatus},
        parameters::{ContextParametersExt, GlobalParams, Params},
        rate_fns::load_rate_fns,
    };
    use std::any::TypeId;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum MandatoryIntervention {
        Partial,
        Full,
        NoEffect,
    }
    impl_property!(
        MandatoryIntervention,
        Person,
        default_const = MandatoryIntervention::NoEffect
    );

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum InfectiousnessProportion {
        None,
        Partial,
    }
    impl_property!(
        InfectiousnessProportion,
        Person,
        default_const = InfectiousnessProportion::None
    );

    pub const SUSCEPTIBLE_PARTIAL: f64 = 0.8;
    pub const INFECTIOUS_PARTIAL: f64 = 0.5;

    fn setup() -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(GlobalParams, Params::default())
            .unwrap();
        let &Params { seed, .. } = context.get_params();
        context.init_random(seed);

        load_rate_fns(&mut context).unwrap();

        context
            .store_transmission_modifier_values(
                InfectionStatus::Susceptible,
                &[
                    (MandatoryIntervention::Partial, SUSCEPTIBLE_PARTIAL),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();
        context
            .store_transmission_modifier_values(
                InfectionStatus::Infectious,
                &[
                    (MandatoryIntervention::Partial, INFECTIOUS_PARTIAL),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();
        context
            .store_transmission_modifier_values(
                InfectionStatus::Infectious,
                &[(InfectiousnessProportion::Partial, INFECTIOUS_PARTIAL)],
            )
            .unwrap();
        context
    }

    #[test]
    fn test_register_modifier_values() {
        let mut context = Context::new();
        let relative_effect = 0.8;

        context
            .store_transmission_modifier_values(
                InfectionStatus::Susceptible,
                &[
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();

        // Add people with different intervention statuses
        let partial_id = context
            .add_entity(with!(Person, MandatoryIntervention::Partial, Age(30)))
            .unwrap();
        let full_id = context
            .add_entity(with!(Person, MandatoryIntervention::Full, Age(30)))
            .unwrap();

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context.get_data(TransmissionModifierPlugin);
        let modifier_map = modifier_container
            .transmission_modifier_map
            .get(&InfectionStatus::Susceptible)
            .unwrap();

        // Check that the modifier map contains the expected values
        assert_eq!(modifier_map.len(), 1);

        let modifier_fn = modifier_map
            .get(&TypeId::of::<
                PersonPropertyTransmissionModifier<MandatoryIntervention>,
            >())
            .unwrap();

        // Check that the modifier function returns the expected value
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, partial_id),
            relative_effect,
            0.0
        );
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, full_id),
            0.0,
            0.0
        );
    }

    #[test]
    fn test_register_modifier_value_overwrite() {
        let mut context = Context::new();
        let relative_effect = 0.8;
        let t1: PersonPropertyTransmissionModifier<MandatoryIntervention> =
            PersonPropertyTransmissionModifier {
                transmission_modifier_map: [
                    (MandatoryIntervention::Partial, relative_effect - 0.2),
                    (MandatoryIntervention::Full, 0.2),
                ]
                .into_iter()
                .collect(),
            };

        let t2: PersonPropertyTransmissionModifier<MandatoryIntervention> =
            PersonPropertyTransmissionModifier {
                transmission_modifier_map: [
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ]
                .into_iter()
                .collect(),
            };
        assert_eq!(std::any::Any::type_id(&t1), std::any::Any::type_id(&t2));
        // Store initial modifier values to be overwritten
        context
            .store_transmission_modifier_values(
                InfectionStatus::Susceptible,
                &[
                    (MandatoryIntervention::Partial, relative_effect - 0.2),
                    (MandatoryIntervention::Full, 0.2),
                ],
            )
            .unwrap();

        // Overwrite previous modifier values
        context
            .store_transmission_modifier_values(
                InfectionStatus::Susceptible,
                &[
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ],
            )
            .unwrap();

        // Add people with different intervention statuses
        let partial_id = context
            .add_entity(with!(Person, MandatoryIntervention::Partial, Age(30)))
            .unwrap();
        let full_id = context
            .add_entity(with!(Person, MandatoryIntervention::Full, Age(30)))
            .unwrap();

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context.get_data(TransmissionModifierPlugin);
        let modifier_map = modifier_container
            .transmission_modifier_map
            .get(&InfectionStatus::Susceptible)
            .unwrap();

        // Check that the modifier map contains the expected values
        assert_eq!(modifier_map.len(), 1);

        let modifier_fn = modifier_map
            .get(&TypeId::of::<
                PersonPropertyTransmissionModifier<MandatoryIntervention>,
            >())
            .unwrap();

        // Check that the modifier function returns the expected value of the overwritten registration
        // The log should also be checked here to make sure the overwrite is recorded
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, partial_id),
            relative_effect,
            0.0
        );
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, full_id),
            0.0,
            0.0
        );
    }

    #[test]
    fn test_register_modifier_value_invalid() {
        let mut context = Context::new();

        // Attempt to store invalid modifier values
        let result = context.store_transmission_modifier_values(
            InfectionStatus::Susceptible,
            &[
                (MandatoryIntervention::Partial, 1.2), // Invalid value
                (MandatoryIntervention::Full, 0.0),
            ],
        );

        match result.err() {
            Some(ModelError::ModelError(msg)) => {
                assert_eq!(
                    msg,
                    "Scalar modifier values stored must be between 0.0 and 1.0. Value 1.2 for Partial is not."
                );
            }
            Some(ue) => panic!(
                "Expected an error that Partial attempts to store an invalid modifier. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, seeded infections with no errors."),
        }
    }

    #[test]
    fn test_register_modifier_value_duplicate() {
        let mut context = Context::new();

        // Attempt to store duplicate modifier values
        let result = context.store_transmission_modifier_values(
            InfectionStatus::Susceptible,
            &[
                (MandatoryIntervention::Partial, 0.8),
                (MandatoryIntervention::Partial, 0.5), // Duplicate value
            ],
        );

        match result.err() {
            Some(ModelError::ModelError(msg)) => {
                assert_eq!(
                    msg,
                    "Duplicate values provided in modifier key Values 0.8 and 0.5 were both attempted to be registered to key Partial"
                );
            }
            Some(ue) => panic!(
                "Expected an error that Partial attempts to store a duplicate modifier. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, seeded infections with no errors."),
        }
    }

    #[derive(Debug)]
    struct AgeModifier {
        age_multiplier: f64,
    }

    impl TransmissionModifier for AgeModifier {
        #[allow(clippy::cast_precision_loss)]
        fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64 {
            let age = context.get_property::<Person, Age>(person_id);
            age.0 as f64 * self.age_multiplier
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_register_arbitrary_modifier_fn() {
        let mut context = Context::new();
        let aged_42 = context.add_entity(with!(Person, Age(42))).unwrap();

        context.register_transmission_modifier_fn(
            InfectionStatus::Susceptible,
            AgeModifier {
                age_multiplier: 0.01,
            },
        );

        // Container should now be initialized, safe to unwrap()
        let modifier_container = context.get_data(TransmissionModifierPlugin);
        let modifier_map = modifier_container
            .transmission_modifier_map
            .get(&InfectionStatus::Susceptible)
            .unwrap();

        let modifier_fn = modifier_map.get(&TypeId::of::<AgeModifier>()).unwrap();
        // Check that the modifier function returns the expected value
        assert_almost_eq!(
            modifier_fn.get_relative_transmission(&context, aged_42),
            0.42,
            0.0
        );
    }

    #[test]
    fn test_setup_modifier_values_registration_susceptible() {
        let mut context = setup();

        let person_id_partial = context
            .add_entity(with!(Person, MandatoryIntervention::Partial, Age(30)))
            .unwrap();
        let person_id_full = context
            .add_entity(with!(Person, MandatoryIntervention::Full, Age(30)))
            .unwrap();
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id_partial),
            SUSCEPTIBLE_PARTIAL,
            0.0
        );
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id_full),
            0.0,
            0.0
        );
    }

    #[test]
    fn test_setup_modifier_values_registration_infectious() {
        let mut context = setup();

        let infectious_id = context
            .add_entity(with!(Person, MandatoryIntervention::Partial, Age(30)))
            .unwrap();
        context.infect_person(infectious_id, None, None);
        assert_almost_eq!(
            context.get_relative_total_transmission(infectious_id),
            INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    fn test_default_aggregator_from_container() {
        let mut context = setup();

        let person_id = context
            .add_entity(with!(
                Person,
                MandatoryIntervention::Partial,
                InfectiousnessProportion::Partial,
                Age(30)
            ))
            .unwrap();

        context.infect_person(person_id, None, None);

        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );
    }

    #[test]
    // Test that the default aggregator works correctly when person properties change
    fn test_get_relative_total_transmission() {
        let mut context = setup();

        let person_id = context
            .add_entity(with!(Person, MandatoryIntervention::Partial, Age(30)))
            .unwrap();
        context.infect_person(person_id, None, None);

        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_property(person_id, InfectiousnessProportion::Partial);
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
            0.0
        );

        context.set_property(person_id, InfectiousnessProportion::None);
        assert_almost_eq!(
            context.get_relative_total_transmission(person_id),
            INFECTIOUS_PARTIAL,
            0.0
        );
    }
}
