// use ixa::{
//     Context, PluginContext, define_data_plugin, prelude::Property, trace
// };
// use std::{any::TypeId, collections::HashMap};

// use crate::{error::ModelError, infectiousness_manager::InfectionStatus, settings::{Person, PersonId, SETTING_COUNT}};

// /// Defines a transmission modifier that is used to modify the transmissiveness or susceptibility
// /// of a person based on their infection status.
// // We require `Debug` for easy logging of the trait so the user can see what is happening.
// pub trait ItineraryModifier: std::fmt::Debug + 'static {
//     /// Return the relative potential for infection (transmissiveness or susceptibility) for a person
//     /// based on their infection status.
//     fn get_itinerary_modifier(&self, context: &Context, person_id: PersonId) -> [f64; SETTING_COUNT];

//     /// For debugging purposes. The name of the transmission modifier. The default implementation
//     /// returns the `Debug` representation of the transmission modifier struct on which this trait
//     /// is implemented.
//     fn get_name(&self) -> String {
//         format!("{self:?}")
//     }
// }

// // A type alias for the type of the transmission modifiers specified via a hashmap of person
// // property values and floats -- i.e., `modifier_key: &[(P::Value, f64)]`
// type PersonPropertyModifier<P> = (
//     P,
//     // Use fully qualified syntax for the associated type because type aliases do not have type checking
//     HashMap<<P as Property<Person>>::CanonicalValue, [f64; SETTING_COUNT]>,
// );

// impl<P> ItineraryModifier for PersonPropertyModifier<P>
// where
//     // All person properties implement `Debug` and are static
//     P: Property<Person> + std::fmt::Debug + 'static,
//     // For now, this limits us to person property values that are not floats for use in the
//     // transmisison modifier map convienience method.
//     P::CanonicalValue: std::hash::Hash + Eq,
// {
//     fn get_itinerary_modifier(&self, context: &Context, person_id: PersonId) -> [f64; SETTING_COUNT] {
//         let (person_property, modifier_map) = self;
//         let property_val = context.get_person_property(person_id, *person_property);
//         // Return the corresponding value from the map, or 1.0 if not found
//         let mut result = [1.0; SETTING_COUNT];
//         for i in 0..SETTING_COUNT {
//             result[i] = *modifier_map.get(&property_val).unwrap_or(&[1.0; SETTING_COUNT])[i];
//         }
//         result
//     }
//     fn get_name(&self) -> String {
//         format!("{:?}", self.0)
//     }
// }

// #[derive(Default)]
// struct ItineraryModifierContainer {
//     itinerary_modifier_map:
//         HashMap<TypeId, Box<dyn ItineraryModifier>>,
// }

// define_data_plugin!(
//     ItineraryModifierPlugin,
//     ItineraryModifierContainer,
//     ItineraryModifierContainer::default()
// );

// pub trait ContextItineraryModifierExt: PluginContext {
//     /// Register a generic itinerary modifier for a specific infection status.
//     fn register_itinerary_modifier_fn<T: ItineraryModifier>(
//         &mut self,
//         itinerary_modifier: T,
//     ) {
//         // Box the itinerary modifier to store it in the map
//         // Itinerary modifiers must implement debug so that we can more easily log their addition
//         let name = itinerary_modifier.get_name();
//         let boxed_itinerary_modifier = Box::new(itinerary_modifier);

//         // Insert the boxed function into the itinerary modifier map, using entry to handle unititialized keys
//         if let Some(_modifier_fxn) = self
//             .get_data_mut(ItineraryModifierPlugin)
//             .itinerary_modifier_map
//             .insert(TypeId::of::<T>(), boxed_itinerary_modifier)
//         {
//             trace!("Overwriting existing itinerary modifier function for itinerary modifier {name}");
//         }
//     }

//     /// Register a transmission modifier that depends solely on the value of one person property.
//     /// The function accepts a relative transmission potential key, which is a slice of tuples that
//     /// associate values of a specified person property with the relativ etransmission potential of
//     /// a person with that property value. All floats declared in this fashion must be between zero
//     /// and one and represent the proportion of infectiousness or susceptiblity remaining if a
//     /// modifier is active.
//     ///
//     /// Any modifiers based on efficacy (e.g. facemask transmission prevention) should be
//     /// subtracted from 1.0 for effect on relative transmission potential.
//     ///
//     /// Internally, this method registers a transmission modifier function that returns the float
//     /// value associated the person's property value in the
//     /// `relative_transmission_potential_multipliers` key.
//     #[allow(dead_code)]
//     fn store_transmission_modifier_values<P: Property + std::fmt::Debug + 'static>(
//         &mut self,
//         infection_status: InfectionStatusValue,
//         person_property: P,
//         relative_transmission_potential_multipliers: &[(P::Value, f64)],
//     ) -> Result<(), ModelError>
//     where
//         P::Value: std::hash::Hash + Eq,
//     {
//         // Convert modifiers to HashMap
//         let mut modifier_map = HashMap::new();
//         for &(key, value) in relative_transmission_potential_multipliers {
//             if !(0.0..=1.0).contains(&value) {
//                 return Err(IxaError::IxaError(
//                     "Scalar modifier values stored must be between 0.0 and 1.0. ".to_string()
//                         + &format!("Value {value} for {person_property:?}::{key:?} is not."),
//                 ));
//             }

//             if let Some(old_value) = modifier_map.insert(key, value) {
//                 return Err(IxaError::IxaError(
//                     "Duplicate values provided in modifier key ".to_string()
//                         + &format!("Values {old_value} and {value} were both attempted to be registered to key {person_property:?}::{key:?}"),
//                 ));
//             }
//         }

//         // Register a default function to simply map floats with T::Values
//         self.register_transmission_modifier_fn(infection_status, (person_property, modifier_map));
//         Ok(())
//     }

//     /// Get the relative potential for infection (infectiousness or susceptibility) for a person
//     /// based on their infection status based on all registered modifiers. Queries all registered
//     /// modifier functions and evaluates them based on the person's properties. Multiplies them
//     /// together to get the total relative transmission modifier for the person.
//     /// Returns 1.0 if no modifiers are registered for the person's infection status.
//     fn get_relative_total_transmission(&self, person_id: PersonId) -> f64;
// }

// impl ContextTransmissionModifierExt for Context {
//     fn get_relative_total_transmission(&self, person_id: PersonId) -> f64 {
//         let infection_status = self.get_person_property(person_id, InfectionStatus);

//         let transmission_modifier_plugin = self.get_data(TransmissionModifierPlugin);
//         if let Some(transmission_modifier_map) = transmission_modifier_plugin
//             .transmission_modifier_map
//             .get(&infection_status)
//         {
//             // Calculate the relative modifier for each registered function and multiply them
//             // together to get the total relative transmission modifier for the person
//             transmission_modifier_map
//                 .iter()
//                 .fold(1.0, |agg, (_type_id, transmission_modifier)| {
//                     agg * transmission_modifier.get_relative_transmission(self, person_id)
//                 })
//         } else {
//             // If the infection status is not found in the map, return 1.0
//             1.0
//         }
//     }
// }

use std::any::TypeId;

use ixa::{HashMap, prelude::*, CanonicalValue};
use serde::{Serialize};
use strum::IntoEnumIterator;

use crate::{ContextParametersExt, population_loader::{ItineraryRatios, Person, PersonId, SettingIds, Student, Worker}, settings::{ContextSettingExt, SettingCategory}};

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifier {
    pub ranking: usize,
    pub itinerary_ratios: ItineraryRatios,
}

#[derive(Default)]
pub struct ItineraryModifiers {
    modifier: HashMap<(TypeId, CanonicalValue), ItineraryModifier>,
}

impl ItineraryModifiers {
    pub fn new() -> Self {
        Self {
            modifier: HashMap::default(),
        }
    }
    // fn update_dominant_modifier(&mut self, person: PersonId) {
    //     if let Some(modifiers) = self.modifier.get(&person) {
    //         if let Some(max_modifier) = modifiers.iter().max_by_key(|m| m.ranking) {
    //             self.dominant_modifier.insert(person, *max_modifier);
    //         }
    //     }
    // }

    pub fn add_itinerary_modifier<T: Property<Person>>(&mut self, itinerary_modifier: ItineraryModifier) {
        self.modifier.insert(TypeId::of::<T>(), itinerary_modifier);
    }

    pub fn remove_itinerary_modifier<T: Property<Person>>(&mut self) {
        self.modifier.remove(&TypeId::of::<T>());
    }
}

define_data_plugin!(ItineraryModifiersPlugin, ItineraryModifiers, |context| {
    let mut itinerary_modifiers =ItineraryModifiers::default();
    let person_iter = context.get_entity_iterator::<Person>();
    for person_id in person_iter {
        let itinerary_ratios: ItineraryRatios = context.get_property(person_id);
        let itinerary_modifier = ItineraryModifier {
            ranking: 0,
            itinerary_ratios,
        };
        itinerary_modifiers.add_itinerary_modifier<Alive(True)>(person_id, itinerary_modifier);
        itinerary_modifiers.update_dominant_modifier(person_id);
    }
    itinerary_modifiers
});



pub trait ContextItineraryModifierExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt {
    fn register_itinerary_modifier(&mut self, person: PersonId, itinerary_modifier: ItineraryModifier) {
        let container = self.get_data_mut(ItineraryModifiersPlugin);
        container.add_itinerary_modifier(person, itinerary_modifier);
        self.implement_dominant_modifier(person);
    }

    fn remove_itinerary_modifier(&mut self, person: PersonId, itinerary_modifier: ItineraryModifier) {
        let container = self.get_data_mut(ItineraryModifiersPlugin);
        container.remove_itinerary_modifier(person, itinerary_modifier);
        self.implement_dominant_modifier(person);
    }

    fn implement_dominant_modifier(&mut self, person: PersonId) {
        let dominant_itinerary_ratios = {
            let container = self.get_data_mut(ItineraryModifiersPlugin);
            let dominant = container.dominant_modifier.get(&person).map(|m| m.itinerary_ratios.clone()).unwrap();
            dominant
        };
        let previous_dominant_itinerary_ratios = self.get_property::<Person, ItineraryRatios>(person);
        self.set_property::<Person, ItineraryRatios>(
                person,
                dominant_itinerary_ratios.clone(),
        );
        for category in SettingCategory::iter() {
            if let Some(setting_id) = self.get_property::<Person, SettingIds>(person).setting_ids[category] {
                let previous_itinerary_ratio = previous_dominant_itinerary_ratios.itinerary_ratios[category];
                let dominant_itinerary_ratio = dominant_itinerary_ratios.itinerary_ratios[category];
                if previous_itinerary_ratio == 0.0 && dominant_itinerary_ratio != 0.0 {
                    let _ = self.increment_setting_size(setting_id, person);
                }
                if previous_itinerary_ratio != 0.0 && dominant_itinerary_ratio == 0.0 {
                    let _ = self.decrement_setting_size(setting_id, person);
                }
            }
        }
    }

    fn implement_school_closure(&mut self, start: f64, end: f64) {
        let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            }
        };

        let school_closure_work_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.5, 0.25, 0.0, 0.25],
            }
        };

        for person_id in self.get_entity_iterator::<Person>() {
            let student = self.get_property::<Person, Student>(person_id);
            let worker = self.get_property::<Person, Worker>(person_id);
            let school_closure_work_modifier_start = school_closure_work_modifier.clone();
            let school_closure_work_modifier_end = school_closure_work_modifier.clone();
            let school_closure_modifier_start = school_closure_modifier.clone();
            let school_closure_modifier_end = school_closure_modifier.clone();
            if student.0 {
                if worker.0 {
                    self.add_plan(start, move |context| {
                        context.register_itinerary_modifier(person_id, school_closure_work_modifier_start.clone());
                    });
                    self.add_plan(end, move |context| {
                        context.remove_itinerary_modifier(person_id, school_closure_work_modifier_end.clone());
                    });
                } else {
                    self.add_plan(start, move |context| {
                        context.register_itinerary_modifier(person_id, school_closure_modifier_start.clone());
                    });
                    self.add_plan(end, move |context| {
                        context.remove_itinerary_modifier(person_id, school_closure_modifier_end.clone());
                    });
                }
            }
        }
    }

}
impl ContextItineraryModifierExt for Context {}
pub fn init(_context: &mut Context) -> Result<(), IxaError> {
    // context.implement_school_closure(1.0, 40.0);
    Ok(())
}
