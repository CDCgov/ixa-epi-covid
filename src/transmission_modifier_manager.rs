use ixa::prelude::*;
use std::{any::{Any, TypeId}, collections::HashMap};

use crate::{error::ModelError, infectiousness_manager::InfectionStatus, settings::{Person, PersonId}};

/// Defines a transmission modifier that is used to modify the transmissiveness or susceptibility
/// of a person based on their infection status.
// We require `Debug` for easy logging of the trait so the user can see what is happening.
pub trait TransmissionModifier: std::fmt::Debug + 'static {
    /// Return the relative potential for infection (transmissiveness or susceptibility) for a person
    /// based on their infection status.
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> Vec<f64>;

}

trait TransmissionModifierStorage: TransmissionModifier {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: TransmissionModifier> TransmissionModifierStorage for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug)]
struct PersonPropertyTransmissionModifier<P> {
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    transmission_modifier_map: HashMap<P, Vec<f64>>,
}

impl<P> TransmissionModifier for PersonPropertyTransmissionModifier<P>
where
    P: IndexableProperty<Person>
{
    fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> Vec<f64> {
        let property_val = context.get_property::<Person, P>(person_id);

        self.transmission_modifier_map
            .get(&property_val)
            .iter()
            .flat_map(|modifiers| modifiers.iter())
            .copied()
            .collect()
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

pub trait ContextTransmissionModifierExt: PluginContext {
    /// Register a generic transmission modifier for a specific infection status.
    fn register_transmission_modifier_fn<T: TransmissionModifier>(
        &mut self,
        infection_status: InfectionStatus,
        transmission_modifier: T,
    ) {
        // Box the transmission modifier to store it in the map
        // Transmission modifiers must implement debug so that we can more easily log their addition
        let name = format!("{transmission_modifier:?}");
        let boxed_transmission_modifier = Box::new(transmission_modifier);

        // Insert the boxed function into the transmission modifier map, using entry to handle unititialized keys
        if let Some(_modifier_fxn) = self
            .get_data_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .insert(TypeId::of::<T>(), boxed_transmission_modifier)
        {
            trace!("Overwriting existing transmission modifier function for infection status {infection_status:?} and transmission modifier {name}");
        }
    }

    /// Append a scalar modifier for an infection status and person property value.
    /// Values must be between zero and one and represent the proportion of
    /// infectiousness or susceptibility remaining. Subtract efficacy from 1.0 first.
    fn register_transmission_modifier<P>(
        &mut self,
        infection_status: InfectionStatus,
        person_property: P,
        relative_transmission_potential_multipliers: f64,
    ) -> Result<(), ModelError>
    where
        P: IndexableProperty<Person>,
    {
        if !(0.0..=1.0).contains(&relative_transmission_potential_multipliers) {
            return Err(ModelError::ModelError(
                "Scalar modifier values stored must be between 0.0 and 1.0. ".to_string()
                    + &format!("Value {relative_transmission_potential_multipliers} for {person_property:?} is not."),
            ));
        }

        let storage = self
            .get_data_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .entry(infection_status)
            .or_default()
            .entry(TypeId::of::<PersonPropertyTransmissionModifier<P>>())
            .or_insert_with(|| {
                Box::new(PersonPropertyTransmissionModifier::<P> {
                    transmission_modifier_map: HashMap::new(),
                })
            });

        let modifier_map = storage
            .as_any_mut()
            .downcast_mut::<PersonPropertyTransmissionModifier<P>>()
            .expect("transmission modifier storage has the wrong type");

        modifier_map
            .transmission_modifier_map
            .entry(person_property)
            .or_default()
            .push(relative_transmission_potential_multipliers);
        Ok(())
    }

    /// Remove all scalar modifiers for an infection status and person property value.
    fn remove_transmission_modifier<P: IndexableProperty<Person>>(
        &mut self,
        infection_status: InfectionStatus,
        property_value: P,
    ) -> Option<Vec<f64>> {
        let storage = self
            .get_data_mut(TransmissionModifierPlugin)
            .transmission_modifier_map
            .get_mut(&infection_status)?
            .get_mut(&TypeId::of::<PersonPropertyTransmissionModifier<P>>())?;

        storage
            .as_any_mut()
            .downcast_mut::<PersonPropertyTransmissionModifier<P>>()
            .expect("transmission modifier storage has the wrong type")
            .transmission_modifier_map
            .remove(&property_value)
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
                    agg * transmission_modifier
                        .get_relative_transmission(self, person_id)
                        .into_iter()
                        .product::<f64>()
                })
        } else {
            // If the infection status is not found in the map, return 1.0
            1.0
        }
    }
}

#[cfg(test)]
mod test {
    use ixa::{
        prelude::*,
    };
    use serde::{Deserialize, Serialize};

    use super::{
        ContextTransmissionModifierExt,
        TransmissionModifierPlugin,
    };
    use crate::Age;
use crate::settings::Person;
use crate::transmission_modifier_manager::PersonPropertyTransmissionModifier;
use crate::{
        infectiousness_manager::{InfectionStatus},
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
    impl_property!(MandatoryIntervention, Person);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum InfectiousnessProportion {
        None,
        Partial,
    }
    impl_property!(InfectiousnessProportion, Person, default_const = InfectiousnessProportion::None);
    #[allow(dead_code)]
    pub const SUSCEPTIBLE_PARTIAL: f64 = 0.8;
    pub const INFECTIOUS_PARTIAL: f64 = 0.5;
    #[allow(dead_code)]
    fn setup() -> Context {
        let mut context = Context::new();
        context
            .set_global_property_value(GlobalParams, Params::default())
            .unwrap();
        let &Params { seed, .. } = context.get_params();
        context.init_random(seed);

        load_rate_fns(&mut context).unwrap();

        context
            .register_transmission_modifier(
                InfectionStatus::Susceptible,
                MandatoryIntervention::Partial,
                SUSCEPTIBLE_PARTIAL,
            )
            .unwrap();
        let _ =context.register_transmission_modifier(InfectionStatus::Susceptible, MandatoryIntervention::Full, 0.0);
        context
            .register_transmission_modifier(
                InfectionStatus::Infectious,
                MandatoryIntervention::Partial,
                INFECTIOUS_PARTIAL,
            )
            .unwrap();
        let _ = context.register_transmission_modifier(InfectionStatus::Infectious, MandatoryIntervention::Full, 0.0);

        context
    }

    #[test]
    fn test_register_modifier_values() {
        let mut context = Context::new();
        let relative_effect = 0.8;

        context
            .register_transmission_modifier(
                InfectionStatus::Susceptible,
                MandatoryIntervention::Partial,
                relative_effect,
            )
            .unwrap();
        context
            .register_transmission_modifier(
                InfectionStatus::Susceptible,
                MandatoryIntervention::Full,
                0.0,
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
        assert_eq!(
            modifier_fn.get_relative_transmission(&context, partial_id),
            vec![relative_effect]
        );
        assert_eq!(
            modifier_fn.get_relative_transmission(&context, full_id),
            vec![0.0]
        );
    }

    // #[test]
    // fn test_register_modifier_value_overwrite() {
    //     let mut context = Context::new();
    //     let relative_effect = 0.8;

    //     // Store initial modifier values to be overwritten
    //     context
    //         .store_transmission_modifier_values(
    //             InfectionStatusValue::Susceptible,
    //             MandatoryInterventionStatus,
    //             &[
    //                 (MandatoryIntervention::Partial, relative_effect - 0.2),
    //                 (MandatoryIntervention::Full, 0.2),
    //             ],
    //         )
    //         .unwrap();

    //     // Overwrite previous modifier values
    //     context
    //         .store_transmission_modifier_values(
    //             InfectionStatusValue::Susceptible,
    //             MandatoryInterventionStatus,
    //             &[
    //                 (MandatoryIntervention::Partial, relative_effect),
    //                 (MandatoryIntervention::Full, 0.0),
    //             ],
    //         )
    //         .unwrap();

    //     // Add people with different intervention statuses
    //     let partial_id = context
    //         .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
    //         .unwrap();
    //     let full_id = context
    //         .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
    //         .unwrap();

    //     // Container should now be initialized, safe to unwrap()
    //     let modifier_container = context.get_data(TransmissionModifierPlugin);
    //     let modifier_map = modifier_container
    //         .transmission_modifier_map
    //         .get(&InfectionStatusValue::Susceptible)
    //         .unwrap();

    //     // Check that the modifier map contains the expected values
    //     assert_eq!(modifier_map.len(), 1);

    //     let modifier_fn = modifier_map
    //         .get(&TypeId::of::<
    //             PersonPropertyModifier<MandatoryInterventionStatus>,
    //         >())
    //         .unwrap();

    //     // Check that the modifier function returns the expected value of the overwritten registration
    //     // The log should also be checked here to make sure the overwrite is recorded
    //     assert_almost_eq!(
    //         modifier_fn.get_relative_transmission(&context, partial_id),
    //         relative_effect,
    //         0.0
    //     );
    //     assert_almost_eq!(
    //         modifier_fn.get_relative_transmission(&context, full_id),
    //         0.0,
    //         0.0
    //     );
    // }

    // #[test]
    // fn test_register_modifier_value_invalid() {
    //     let mut context = Context::new();

    //     // Attempt to store invalid modifier values
    //     let result = context.store_transmission_modifier_values(
    //         InfectionStatusValue::Susceptible,
    //         MandatoryInterventionStatus,
    //         &[
    //             (MandatoryIntervention::Partial, 1.2), // Invalid value
    //             (MandatoryIntervention::Full, 0.0),
    //         ],
    //     );

    //     match result.err() {
    //         Some(IxaError::IxaError(msg)) => {
    //             assert_eq!(
    //                 msg,
    //                 "Scalar modifier values stored must be between 0.0 and 1.0. Value 1.2 for MandatoryInterventionStatus::Partial is not."
    //             );
    //         }
    //         Some(ue) => panic!(
    //             "Expected an error that Partial attempts to store an invalid modifier. Instead got {:?}",
    //             ue.to_string()
    //         ),
    //         None => panic!("Expected an error. Instead, seeded infections with no errors."),
    //     }
    // }

    // #[test]
    // fn test_register_modifier_value_duplicate() {
    //     let mut context = Context::new();

    //     // Attempt to store duplicate modifier values
    //     let result = context.store_transmission_modifier_values(
    //         InfectionStatusValue::Susceptible,
    //         MandatoryInterventionStatus,
    //         &[
    //             (MandatoryIntervention::Partial, 0.8),
    //             (MandatoryIntervention::Partial, 0.5), // Duplicate value
    //         ],
    //     );

    //     match result.err() {
    //         Some(IxaError::IxaError(msg)) => {
    //             assert_eq!(
    //                 msg,
    //                 "Duplicate values provided in modifier key Values 0.8 and 0.5 were both attempted to be registered to key MandatoryInterventionStatus::Partial"
    //             );
    //         }
    //         Some(ue) => panic!(
    //             "Expected an error that Partial attempts to store a duplicate modifier. Instead got {:?}",
    //             ue.to_string()
    //         ),
    //         None => panic!("Expected an error. Instead, seeded infections with no errors."),
    //     }
    // }

    // #[derive(Debug)]
    // struct AgeModifier {
    //     age_multiplier: f64,
    // }

    // impl TransmissionModifier for AgeModifier {
    //     #[allow(clippy::cast_precision_loss)]
    //     fn get_relative_transmission(&self, context: &Context, person_id: PersonId) -> f64 {
    //         let age = context.get_person_property(person_id, Age);
    //         age as f64 * self.age_multiplier
    //     }
    // }

    // #[test]
    // #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    // fn test_register_arbitrary_modifier_fn() {
    //     let mut context = Context::new();
    //     let aged_42 = context.add_person((Age, 42)).unwrap();

    //     context.register_transmission_modifier_fn(
    //         InfectionStatusValue::Susceptible,
    //         AgeModifier {
    //             age_multiplier: 0.01,
    //         },
    //     );

    //     // Container should now be initialized, safe to unwrap()
    //     let modifier_container = context.get_data(TransmissionModifierPlugin);
    //     let modifier_map = modifier_container
    //         .transmission_modifier_map
    //         .get(&InfectionStatusValue::Susceptible)
    //         .unwrap();

    //     let modifier_fn = modifier_map.get(&TypeId::of::<AgeModifier>()).unwrap();
    //     // Check that the modifier function returns the expected value
    //     assert_almost_eq!(
    //         modifier_fn.get_relative_transmission(&context, aged_42),
    //         0.42,
    //         0.0
    //     );
    // }

    // #[test]
    // fn test_setup_modifier_values_registration_susceptible() {
    //     let mut context = setup();

    //     let person_id_partial = context
    //         .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
    //         .unwrap();
    //     let person_id_full = context
    //         .add_person((MandatoryInterventionStatus, MandatoryIntervention::Full))
    //         .unwrap();
    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(person_id_partial),
    //         SUSCEPTIBLE_PARTIAL,
    //         0.0
    //     );
    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(person_id_full),
    //         0.0,
    //         0.0
    //     );
    // }

    // #[test]
    // fn test_setup_modifier_values_registration_infectious() {
    //     let mut context = setup();

    //     let infectious_id = context
    //         .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
    //         .unwrap();
    //     context.infect_person(infectious_id, None, None, None);
    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(infectious_id),
    //         INFECTIOUS_PARTIAL,
    //         0.0
    //     );
    // }

    // #[test]
    // fn test_default_aggregator_from_container() {
    //     let mut context = setup();

    //     let person_id = context
    //         .add_person((
    //             (MandatoryInterventionStatus, MandatoryIntervention::Partial),
    //             (
    //                 InfectiousnessProportionStatus,
    //                 InfectiousnessProportion::Partial,
    //             ),
    //         ))
    //         .unwrap();

    //     context.infect_person(person_id, None, None, None);

    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(person_id),
    //         INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
    //         0.0
    //     );
    // }

    // #[test]
    // // Test that the default aggregator works correctly when person properties change
    // fn test_get_relative_total_transmission() {
    //     let mut context = setup();

    //     let person_id = context
    //         .add_person((MandatoryInterventionStatus, MandatoryIntervention::Partial))
    //         .unwrap();
    //     context.infect_person(person_id, None, None, None);

    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(person_id),
    //         INFECTIOUS_PARTIAL,
    //         0.0
    //     );

    //     context.set_person_property(
    //         person_id,
    //         InfectiousnessProportionStatus,
    //         InfectiousnessProportion::Partial,
    //     );
    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(person_id),
    //         INFECTIOUS_PARTIAL * INFECTIOUS_PARTIAL,
    //         0.0
    //     );

    //     context.set_person_property(
    //         person_id,
    //         InfectiousnessProportionStatus,
    //         InfectiousnessProportion::None,
    //     );
    //     assert_almost_eq!(
    //         context.get_relative_total_transmission(person_id),
    //         INFECTIOUS_PARTIAL,
    //         0.0
    //     );
    // }
}
