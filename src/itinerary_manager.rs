use ixa::{
    prelude::*,
};
use serde::Serialize;
use std::{any::{TypeId}, collections::HashMap};

use crate::{Age, settings::ItineraryRatios};
use crate::population_loader::{Person, PersonId};

define_rng!(DummyRng);

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifier{
    ranking: usize,
    itinerary_ratios: ItineraryRatios
}

// pub trait ItineraryModifiers: std::fmt::Debug + Any {
//     /// Return the itinerary of a person based on their properties and the current context.
//     #[allow(dead_code)]
//     fn get_itinerary(
//         &self,
//         context: &Context,
//         person_id: PersonId,
//     ) -> Option<ItineraryModifier>;

//     /// For debugging purposes. The name of the itinerary modifier. The default implementation
//     /// returns the `Debug` representation of the itinerary modifier struct on which this trait
//     /// is implemented.
//     fn get_name(&self) -> String {
//         format!("{self:?}")
//     }
// }

// A newtype wrapper for the itinerary modifiers specified via a hashmap of person
// property values and itinerary -- i.e., `modifier_key: &[(P::Value, Vec<ItineraryEntry>)]`
#[allow(dead_code)]
#[derive(Debug)]
struct PersonPropertyModifier<'a, P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    property: P,
    modifiers: HashMap<<P as Property<Person>>::CanonicalValue, ItineraryModifier>,
    _phantom: std::marker::PhantomData<&'a ()>,
}
#[allow(dead_code)]
impl<'a, P> PersonPropertyModifier<'a, P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq,
{
    fn get(&self, key: &P::CanonicalValue) -> Option<&ItineraryModifier> {
        self.modifiers.get(key)
    }
}

// #[allow(dead_code)]
// impl<P> ItineraryModifiers for PersonPropertyModifier<'_, P>
// where
//     P: Property<Person, CanonicalValue = P> + std::fmt::Debug + 'static,
//     P::CanonicalValue: std::hash::Hash + Eq,
// {
//     fn get_itinerary(
//         &self,
//         context: &Context,
//         person_id: PersonId,
//     ) -> Option<ItineraryModifier> {
//         let (_person_property, modifier_map) = self;
//         let property_val = context.get_property::<Person, P>(person_id);
//         match modifier_map.get(&property_val) {
//             Some(value) => Some(*value),
//             None => None,
//         }
//     }
    
//     fn get_name(&self) -> String {
//         format!("{:?}", self.0)
//     }
// }

#[derive(Default)]
struct ItineraryModifierContainer {
    itinerary_modifier_map: HashMap<TypeId, Box<dyn std::any::Any>>,
}

define_data_plugin!(
    ItineraryModifierPlugin,
    ItineraryModifierContainer,
    ItineraryModifierContainer::default()
);

pub trait ContextItineraryModifierExt: PluginContext + ContextEntitiesExt {
    /// Register a generic itinerary modifier.
    fn register_itinerary_modifier<P: Property<Person> + std::fmt::Debug + 'static>(
        &mut self,
        person_property: P,
        person_property_value: P::CanonicalValue,
        itinerary_modifier: ItineraryModifier,
    )
    where
        P::CanonicalValue: std::hash::Hash + Eq,
    {
        // Box the itinerary modifier to store it in the map
        // Itinerary modifiers must implement debug so that we can more easily log their addition
        let person_property_modifier = PersonPropertyModifier {
            property: person_property,
            modifiers: HashMap::from([(person_property_value, itinerary_modifier)]),
            _phantom: std::marker::PhantomData,
        };
        // Insert the boxed function into the itinerary modifier map, using entry to handle unititialized keys
        if let Some(_modifier_fxn) = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .insert(TypeId::of::<P>(), Box::new(person_property_modifier))
        {
            trace!("Overwriting existing itinerary modifier function for and itinerary modifier");
        }
    }

    fn remove_itinerary_modifier_fn<P: Property<Person> + 'static>(&mut self) {
        self.get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .remove(&TypeId::of::<P>());
    }

    // fn store_itinerary_modifier_values<
    //     P: Property<Person, CanonicalValue = P> + std::fmt::Debug + 'static,
    // >(
    //     &mut self,
    //     person_property: P,
    //     person_property_value: P::CanonicalValue,
    //     itinerary_modifier: ItineraryModifier,
    // ) -> Result<(), ModelError>
    // where
    //     P::CanonicalValue: std::hash::Hash + Eq,
    // {
    //     // Convert modifiers to HashMap
    //     let mut modifier_map = HashMap::new();
    //     if let Some(old_value) = modifier_map.insert(person_property_value, itinerary_modifier) {
    //         return Err(ModelError::ModelError(
    //             "Duplicate values provided in modifier key ".to_string()
    //                 + &format!("Values {old_value:?} and {itinerary_modifier:?} were both attempted to be registered to key {person_property:?}::{person_property_value:?}"),
    //         ));
    //     }
    //     self.register_itinerary_modifier_fn((person_property, modifier_map.clone()));
    //     Ok(())
    // }

    fn get_itinerary<P: Property<Person> + std::fmt::Debug + 'static>(&self, person_id: PersonId, person_property: P) 
    where <P as ixa::prelude::Property<Person>>::CanonicalValue: std::hash::Hash + Eq {
        let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
        println!("itinerary modifier map: {:?}", itinerary_modifier_container.itinerary_modifier_map);
        let modifier_key = TypeId::of::<P>();
        let modifier = itinerary_modifier_container.itinerary_modifier_map.get(&modifier_key).unwrap();
        println!("Getting itinerary modifier for person {person_id} and property {person_property:?}");
        println!("Modifier: {:?}", modifier);
        let temp = modifier.downcast_ref::<PersonPropertyModifier<'_, P>>().unwrap();
        let property = temp.property;
        let property_val = temp.modifiers.keys().next().unwrap();
        println!("Property: {:?}", property);
        println!("Property value: {:?}", property_val);
        let age = self.get_property::<Person, P>(person_id);
        println!("Person property value: {:?}", age);
        println!("property == age {}", property == age);
        // match modifier.get(&property_val) {
        //     Some(value) => Some(*value),
        //     None => None,
        // }
    }

    // fn get_dominant_itinerary_modifier(&self, person_id: PersonId) -> Option<ItineraryModifier> {
    //     println!("Getting dominant itinerary modifier for person {person_id}");
    //     let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
    //     let mut dominant_itinerary_modifier: Option<ItineraryModifier> = None;
    //     for itinerary_modifier in itinerary_modifier_container.itinerary_modifier_map.values() {
    //         println!("Checking itinerary modifier: {:?}", itinerary_modifier);
    //         if let Some(modifier) = itinerary_modifier.get_itinerary(self, person_id) {
    //             if let Some(current_dominant) = dominant_itinerary_modifier {
    //                 if modifier.ranking > current_dominant.ranking {
    //                     dominant_itinerary_modifier = Some(modifier);
    //                 }
    //             } else {
    //                 dominant_itinerary_modifier = Some(modifier);
    //             }
    //         }
    //     }
    //     dominant_itinerary_modifier
    // }

}
impl ContextItineraryModifierExt for Context {}

pub fn init(context: &mut Context) {
    let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            }
        };
    context
        .register_itinerary_modifier(
            Age(0),
            Age(10),
            school_closure_modifier,
        );
    let p1 = context.sample_entity(DummyRng, (Age(11),)).unwrap();
    println!("dominant modifier {:?}", context.get_itinerary(p1, Age(11)));
    context.shutdown();
}