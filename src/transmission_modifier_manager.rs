use ixa::prelude::*;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::{
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    intervention_manager::ModifierSpecification,
    itinerary_modifiers::AcceptanceFunction,
    settings::{Person, PersonId},
};

pub struct ScalarTransmissionModifier {
    pub infectiousness_multiplier: f64,
    pub susceptibility_multiplier: f64,
    pub acceptance_function: Option<AcceptanceFunction>,
}

impl ModifierSpecification for ScalarTransmissionModifier {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

struct StoredScalarTransmissionModifier {
    pub multiplier: f64,
    pub acceptance_function: Option<AcceptanceFunction>,
}

impl std::fmt::Debug for StoredScalarTransmissionModifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredScalarTransmissionModifier")
            .field("multiplier", &self.multiplier)
            .field(
                "has_acceptance_function",
                &self.acceptance_function.is_some(),
            )
            .finish()
    }
}

pub trait TransmissionModifier: std::fmt::Debug + 'static {
    fn as_any(&self) -> &dyn Any;
    fn accept(&self, _context: &Context, _person_id: PersonId) -> bool {
        true
    }
}

impl TransmissionModifier for StoredScalarTransmissionModifier {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn accept(&self, context: &Context, person_id: PersonId) -> bool {
        self.acceptance_function
            .as_ref()
            .is_none_or(|acceptance| acceptance(context, person_id))
    }
}

/// Defines a transmission modifier that is used to modify the transmissiveness or susceptibility
/// of a person based on their infection status.
// We require `Debug` for easy logging of the trait so the user can see what is happening.
pub trait TransmissionModifierStorage: std::fmt::Debug + 'static {
    /// Return the relative potential for infection (transmissiveness or susceptibility) for a person
    /// based on their infection status.
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64;
}

// A type alias for the type of the transmission modifiers specified via a hashmap of person
// property values and floats -- i.e., `modifier_key: &[(P::Value, f64)]`
#[derive(Debug)]
struct PersonPropertyTransmissionModifier<P> {
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    transmission_modifier_map: HashMap<P, Box<dyn TransmissionModifier>>,
}

impl<P> TransmissionModifierStorage for PersonPropertyTransmissionModifier<P>
where
    P: IndexableProperty<Person>,
{
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64 {
        let property_val = context.get_property::<Person, P>(person_id);
        // Return the corresponding value from the map, or 1.0 if not found
        self.transmission_modifier_map
            .get(&property_val)
            .map(move |boxed| {
                if let Some(scalar_modifier) = boxed
                    .as_any()
                    .downcast_ref::<StoredScalarTransmissionModifier>()
                {
                    if scalar_modifier.accept(context, person_id) {
                        scalar_modifier.multiplier
                    } else {
                        1.0
                    }
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0)
    }
}

#[derive(Default)]
struct TransmissionModifierContainer {
    transmission_modifier_map:
        HashMap<InfectionStatus, HashMap<TypeId, Box<dyn TransmissionModifierStorage>>>,
}

define_data_plugin!(
    TransmissionModifierPlugin,
    TransmissionModifierContainer,
    TransmissionModifierContainer::default()
);

pub trait ContextTransmissionModifierPrivateExt: PluginContext {
    /// Register a generic transmission modifier for a specific infection status.
    fn insert_transmission_modifier_fn<T: TransmissionModifierStorage>(
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
    /// The function accepts a relative transmission potential key, which is an owned iterable of tuples that
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
    fn store_transmission_modifier_values<P, T>(
        &mut self,
        infection_status: InfectionStatus,
        relative_transmission_potential_multipliers: impl IntoIterator<Item = (P, T)>,
    ) -> Result<(), ModelError>
    where
        P: IndexableProperty<Person>,
        T: TransmissionModifier,
    {
        // Convert modifiers to HashMap
        let mut modifier_map: HashMap<P, Box<dyn TransmissionModifier>> = HashMap::new();
        for (key, value) in relative_transmission_potential_multipliers {
            if let Some(scalar) = value
                .as_any()
                .downcast_ref::<StoredScalarTransmissionModifier>()
                && !(0.0..=1.0).contains(&scalar.multiplier)
            {
                return Err(ModelError::ModelError(format!(
                    "Scalar modifier values stored must be between 0.0 and 1.0. Value {} for {key:?} is not.",
                    scalar.multiplier
                )));
            }
            if let Some(old_value) = modifier_map.get(&key) {
                return Err(ModelError::ModelError(
                    "Duplicate values provided in modifier key ".to_string()
                        + &format!(
                            "Values {:?} and {:?} were both attempted to be registered to key {key:?}",
                            old_value, value
                        ),
                ));
            }
            modifier_map.insert(key, Box::new(value));
        }

        // Register a default function to simply map floats with T::Values
        self.insert_transmission_modifier_fn(
            infection_status,
            PersonPropertyTransmissionModifier {
                transmission_modifier_map: modifier_map,
            },
        );
        Ok(())
    }
}
impl ContextTransmissionModifierPrivateExt for Context {}
pub trait ContextTransmissionModifierExt:
    PluginContext + ContextTransmissionModifierPrivateExt
{
    fn register_transmission_modifier<P>(
        &mut self,
        transmission_modifier: impl IntoIterator<Item = (P, ScalarTransmissionModifier)>,
    ) -> Result<(), ModelError>
    where
        P: IndexableProperty<Person>,
    {
        let mut susceptible_modifiers = Vec::new();
        let mut infectious_modifiers = Vec::new();
        for (key, value) in transmission_modifier {
            let (infectious_acceptance, susceptible_acceptance) = match value.acceptance_function {
                Some(acceptance) => {
                    let acceptance = std::rc::Rc::new(acceptance);
                    let infectious_acceptance = std::rc::Rc::clone(&acceptance);
                    (
                        Some(Box::new(move |context: &Context, person_id: PersonId| {
                            infectious_acceptance(context, person_id)
                        }) as AcceptanceFunction),
                        Some(Box::new(move |context: &Context, person_id: PersonId| {
                            acceptance(context, person_id)
                        }) as AcceptanceFunction),
                    )
                }
                None => (None, None),
            };
            let relative_infectious_multiplier = StoredScalarTransmissionModifier {
                multiplier: value.infectiousness_multiplier,
                acceptance_function: infectious_acceptance,
            };
            let relative_susceptibility_multiplier = StoredScalarTransmissionModifier {
                multiplier: value.susceptibility_multiplier,
                acceptance_function: susceptible_acceptance,
            };
            infectious_modifiers.push((key, relative_infectious_multiplier));
            susceptible_modifiers.push((key, relative_susceptibility_multiplier));
        }
        self.store_transmission_modifier_values(InfectionStatus::Infectious, infectious_modifiers)?;
        self.store_transmission_modifier_values(
            InfectionStatus::Susceptible,
            susceptible_modifiers,
        )?;

        Ok(())
    }

    fn get_transmission_success_probability(&self, infector: PersonId, infectee: PersonId) -> f64 {
        self.get_relative_total_transmission(infector)
            * self.get_relative_total_transmission(infectee)
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
        ContextTransmissionModifierExt, PersonPropertyTransmissionModifier,
        TransmissionModifierPlugin, TransmissionModifierStorage,
    };
    use crate::Age;
    use crate::error::ModelError;
    use crate::settings::{Person, PersonId};
    use crate::transmission_modifier_manager::ContextTransmissionModifierPrivateExt;
    use crate::transmission_modifier_manager::ScalarTransmissionModifier;
    use crate::transmission_modifier_manager::StoredScalarTransmissionModifier;
    use crate::transmission_modifier_manager::TransmissionModifier;
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
            .register_transmission_modifier([
                (
                    MandatoryIntervention::Partial,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: INFECTIOUS_PARTIAL,
                        susceptibility_multiplier: SUSCEPTIBLE_PARTIAL,
                        acceptance_function: None,
                    },
                ),
                (
                    MandatoryIntervention::Full,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: 0.0,
                        acceptance_function: None,
                    },
                ),
            ])
            .unwrap();
        context
            .register_transmission_modifier([(
                InfectiousnessProportion::Partial,
                ScalarTransmissionModifier {
                    infectiousness_multiplier: INFECTIOUS_PARTIAL,
                    susceptibility_multiplier: 0.0,
                    acceptance_function: None,
                },
            )])
            .unwrap();
        context
    }

    #[test]
    fn test_register_modifier_values() {
        let mut context = Context::new();
        let relative_effect = 0.8;

        context
            .register_transmission_modifier([
                (
                    MandatoryIntervention::Partial,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: relative_effect,
                        acceptance_function: None,
                    },
                ),
                (
                    MandatoryIntervention::Full,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: 0.0,
                        acceptance_function: None,
                    },
                ),
            ])
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
        println!("{:?}", modifier_map);
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
                .map(|(k, m)| {
                    (
                        k,
                        Box::new(StoredScalarTransmissionModifier {
                            multiplier: m,
                            acceptance_function: None,
                        }) as Box<dyn TransmissionModifier>,
                    )
                })
                .collect(),
            };

        let t2: PersonPropertyTransmissionModifier<MandatoryIntervention> =
            PersonPropertyTransmissionModifier {
                transmission_modifier_map: [
                    (MandatoryIntervention::Partial, relative_effect),
                    (MandatoryIntervention::Full, 0.0),
                ]
                .into_iter()
                .map(|(k, m)| {
                    (
                        k,
                        Box::new(StoredScalarTransmissionModifier {
                            multiplier: m,
                            acceptance_function: None,
                        }) as Box<dyn TransmissionModifier>,
                    )
                })
                .collect(),
            };
        let t3: PersonPropertyTransmissionModifier<InfectiousnessProportion> =
            PersonPropertyTransmissionModifier {
                transmission_modifier_map: [
                    (InfectiousnessProportion::Partial, relative_effect),
                    (InfectiousnessProportion::None, 0.0),
                ]
                .into_iter()
                .map(|(k, m)| {
                    (
                        k,
                        Box::new(StoredScalarTransmissionModifier {
                            multiplier: m,
                            acceptance_function: None,
                        }) as Box<dyn TransmissionModifier>,
                    )
                })
                .collect(),
            };
        assert_eq!(std::any::Any::type_id(&t1), std::any::Any::type_id(&t2));
        assert_ne!(std::any::Any::type_id(&t1), std::any::Any::type_id(&t3));
        // Store initial modifier values to be overwritten
        context
            .register_transmission_modifier([
                (
                    MandatoryIntervention::Partial,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: relative_effect - 0.2,
                        acceptance_function: None,
                    },
                ),
                (
                    MandatoryIntervention::Full,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: 0.2,
                        acceptance_function: None,
                    },
                ),
            ])
            .unwrap();

        // Overwrite previous modifier values
        context
            .register_transmission_modifier([
                (
                    MandatoryIntervention::Partial,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: relative_effect,
                        acceptance_function: None,
                    },
                ),
                (
                    MandatoryIntervention::Full,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: 0.0,
                        susceptibility_multiplier: 0.0,
                        acceptance_function: None,
                    },
                ),
            ])
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
        let result = context.register_transmission_modifier([
            (
                MandatoryIntervention::Partial,
                ScalarTransmissionModifier {
                    infectiousness_multiplier: 1.2, // Invalid value
                    susceptibility_multiplier: 0.0,
                    acceptance_function: None,
                },
            ),
            (
                MandatoryIntervention::Full,
                ScalarTransmissionModifier {
                    infectiousness_multiplier: 0.0,
                    susceptibility_multiplier: 0.0,
                    acceptance_function: None,
                },
            ),
        ]);

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
        let result = context.register_transmission_modifier([
            (
                MandatoryIntervention::Partial,
                ScalarTransmissionModifier {
                    infectiousness_multiplier: 0.0,
                    susceptibility_multiplier: 0.8,
                    acceptance_function: None,
                },
            ),
            (
                MandatoryIntervention::Partial,
                ScalarTransmissionModifier {
                    infectiousness_multiplier: 0.0,
                    susceptibility_multiplier: 0.5,
                    acceptance_function: None,
                },
            ), // Duplicate value
        ]);

        match result.err() {
            Some(ModelError::ModelError(msg)) => {
                assert_eq!(
                    msg,
                    "Duplicate values provided in modifier key Values StoredScalarTransmissionModifier { multiplier: 0.0, has_acceptance_function: false } and StoredScalarTransmissionModifier { multiplier: 0.0, has_acceptance_function: false } were both attempted to be registered to key Partial"
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

    impl TransmissionModifierStorage for AgeModifier {
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

        context.insert_transmission_modifier_fn(
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
