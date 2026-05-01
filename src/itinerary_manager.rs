use ixa::{
    prelude::*,
};
use serde::Serialize;
use std::{any::{TypeId}, collections::HashMap};

use crate::{Age, population_loader::SchoolId, settings::ItineraryRatios, symptom_status_manager::SymptomStatus};
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

pub trait DummyTrait: std::fmt::Debug + 'static{}

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

    fn get_itinerary<P: Property<Person> + std::fmt::Debug + 'static>(&self, person_property: P) -> Option<ItineraryModifier>
    where 
        <P as ixa::prelude::Property<Person>>::CanonicalValue: std::hash::Hash + Eq 
    {
        let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
        let modifier_key = TypeId::of::<P>();
        let modifier = itinerary_modifier_container.itinerary_modifier_map.get(&modifier_key).unwrap();
        let modifier = modifier.downcast_ref::<PersonPropertyModifier<'_, P>>().unwrap();
        if modifier.property == person_property {
            Some(modifier.modifiers)
        } else {
            None
        }
    }

    fn get_dominant_itinerary_modifier(&self, person_id: PersonId) -> Option<ItineraryModifier> {
        println!("Getting dominant itinerary modifier for person {person_id}");
        let age_modifier = self.get_itinerary(self.get_property::<Person, Age>(person_id));
        let symp_modifier = self.get_itinerary(self.get_property::<Person, SymptomStatus>(person_id));
        let school_modifier = self.get_itinerary(self.get_property::<Person, SchoolId>(person_id));
        let modifiers = [age_modifier, symp_modifier, school_modifier];
        let mut sorted_modifiers: Vec<_> = modifiers.into_iter().flatten().collect();
        sorted_modifiers.sort(); 
        sorted_modifiers.pop()
    }

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
            Age(11),
            school_closure_modifier,
        );
    let p1 = context.sample_entity(DummyRng, (Age(11),)).unwrap();
    println!("dominant modifier {:?}", context.get_itinerary(context.get_property::<Person, Age>(p1)));
    context.shutdown();
}