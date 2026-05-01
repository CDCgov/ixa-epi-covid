use ixa::{
    prelude::*,
};
use serde::Serialize;
use std::{any::{Any, TypeId}, collections::HashMap};

use crate::{Age, settings::ItineraryRatios};
use crate::population_loader::{Person, PersonId};

define_rng!(DummyRng);

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifier{
    ranking: usize,
    itinerary_ratios: ItineraryRatios
}

impl Ord for ItineraryModifier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ranking.cmp(&other.ranking)
    }
}

impl PartialOrd for ItineraryModifier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for ItineraryModifier {}

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

pub trait DummyTrait: std::fmt::Debug  + Any{
    fn as_any(&self) -> &dyn std::any::Any;
    fn get_itinerary(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<ItineraryModifier>;
}

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
    modifiers: ItineraryModifier,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<P> DummyTrait for PersonPropertyModifier<'static, P> 
where 
    P: Property<Person> + std::fmt::Debug, 
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn get_itinerary(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<ItineraryModifier> {
        if self.property == context.get_property::<Person, P>(person_id){
            Some(self.modifiers)
        } else {
            None
        }        
    }
}

#[derive(Default)]
struct ItineraryModifierContainer {
    itinerary_modifier_map: HashMap<TypeId, Box<dyn DummyTrait>>,
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
        itinerary_modifier: ItineraryModifier,
    )
    where
        P::CanonicalValue: std::hash::Hash + Eq,
    {
        // Box the itinerary modifier to store it in the map
        // Itinerary modifiers must implement debug so that we can more easily log their addition
        let person_property_modifier = PersonPropertyModifier {
            property: person_property,
            modifiers: itinerary_modifier,
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

    // fn get_itinerary<P: Property<Person> + std::fmt::Debug + 'static>(&self, person_property: P) -> Option<ItineraryModifier>
    // where 
    //     <P as ixa::prelude::Property<Person>>::CanonicalValue: std::hash::Hash + Eq 
    // {
    //     let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
    //     let modifier_key = TypeId::of::<P>();
    //     let modifier = itinerary_modifier_container.itinerary_modifier_map.get(&modifier_key).unwrap();
    //     let modifier = modifier.as_any();
    //     let modifier = modifier.downcast_ref::<PersonPropertyModifier<'_, P>>().unwrap();
    //     if modifier.property == person_property {
    //         Some(modifier.modifiers)
    //     } else {
    //         None
    //     }
    // }

    fn get_dominant_itinerary_modifier(&self, person_id: PersonId) -> Option<ItineraryModifier>;

}
impl ContextItineraryModifierExt for Context {
    // This needs to be here to have access to the concrete context type for the get_itinerary trait method
    fn get_dominant_itinerary_modifier(&self, person_id: PersonId) -> Option<ItineraryModifier> {
            let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
            let mut dominant_modifier: Option<ItineraryModifier> = None;
            for modifier in itinerary_modifier_container.itinerary_modifier_map.values() {
                let itinerary_modifier = modifier.get_itinerary(self, person_id);
                if let Some(temp) = itinerary_modifier {
                    if let Some(dominant_modifier_temp) = dominant_modifier {
                        if temp > dominant_modifier_temp {
                            dominant_modifier = Some(temp);
                        }
                    } else {
                        dominant_modifier = Some(temp);
                    }
                }
            }
            dominant_modifier      
        }
}

pub fn init(context: &mut Context) {
    let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            }
        };
    context
        .register_itinerary_modifier(
            Age(11),
            school_closure_modifier,
        );
    let p1 = context.sample_entity(DummyRng, (Age(11),)).unwrap();
    println!("dominant modifier {:?}", context.get_dominant_itinerary_modifier(p1));
    context.shutdown();
}