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

pub trait ItineraryModifierTrait: std::fmt::Debug  + Any{
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
struct PersonPropertyItineraryModifier<'a, P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    property: P,
    modifiers: ItineraryModifier,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<P> ItineraryModifierTrait for PersonPropertyItineraryModifier<'static, P> 
where 
    P: Property<Person> + std::fmt::Debug, 
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug {
    
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
    itinerary_modifier_map: HashMap<TypeId, Box<dyn ItineraryModifierTrait>>,
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
        let person_property_modifier = PersonPropertyItineraryModifier {
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

    fn remove_itinerary_modifier<P: Property<Person> + 'static>(&mut self) {
        self.get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .remove(&TypeId::of::<P>());
    }

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
    let p1 = context.sample_entity(DummyRng, (Age(10),)).unwrap();
    println!("dominant modifier {:?}", context.get_dominant_itinerary_modifier(p1));
    context.shutdown();
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::population_loader::Alive;
    use crate::settings::SettingCategory;
    use ixa::HashMap;

    fn setup(
        home_ratio: f64,
        school_ratio: f64,
        work_ratio: f64,
        community_ratio: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, home_ratio),
                (SettingCategory::School, school_ratio),
                (SettingCategory::Work, work_ratio),
                (SettingCategory::Community, community_ratio),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_itinerary_modifier_registration() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
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
        let p1 = context.add_entity::<Person,_>((Age(10),)).unwrap();
        let p2 = context.add_entity::<Person,_>((Age(11),)).unwrap();
        let dominant_modifier_p1 = context.get_dominant_itinerary_modifier(p1);
        let dominant_modifier_p2 = context.get_dominant_itinerary_modifier(p2);
        assert_eq!(dominant_modifier_p1, None);
        assert_eq!(dominant_modifier_p2, Some(school_closure_modifier));
        println!("dominant modifier {:?}", context.get_dominant_itinerary_modifier(p1));
    }

    #[test]
    fn test_itinerary_modifier_removal() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
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
        let p1 = context.add_entity::<Person,_>((Age(11),)).unwrap();
        assert_eq!(context.get_dominant_itinerary_modifier(p1), Some(school_closure_modifier));
        // This would remove all age based itinerary modifiers that is not ideal.
        context.remove_itinerary_modifier::<Age>();
        assert_eq!(context.get_dominant_itinerary_modifier(p1), None);
    }

    #[test]
    fn test_itinerary_modifier_dominance() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
        let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            }
        };
        let work_closure_modifier = ItineraryModifier {
            ranking: 2,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.25, 0.0],
            }
        };
        context
            .register_itinerary_modifier(
                Age(11),
                school_closure_modifier,
            );
        context
            .register_itinerary_modifier(
                Age(11),
                work_closure_modifier,
            );
        let p1 = context.add_entity::<Person,_>((Age(11),)).unwrap();
        assert_eq!(context.get_dominant_itinerary_modifier(p1), Some(work_closure_modifier));
    }

    #[test]
    fn example_with_schools_and_weekends() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
        let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            }
        };

        let weekend_modifier = ItineraryModifier {
            ranking: 2,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.5, 0.0, 0.0, 0.5],
            }
        };
        
        let p1 = context.add_entity::<Person,_>((Age(11),)).unwrap();
        context.add_plan(2.0, move |context| {
            assert_eq!(context.get_dominant_itinerary_modifier(p1), None);
            context
            .register_itinerary_modifier(
                Age(11),
                school_closure_modifier,
            );
            assert_eq!(context.get_dominant_itinerary_modifier(p1), Some(school_closure_modifier));
        });

        context.add_plan(4.0, move |context| {
            context
            .register_itinerary_modifier(
                Alive(true),
                weekend_modifier,
            );
            assert_eq!(context.get_dominant_itinerary_modifier(p1), Some(weekend_modifier));
        });
        context.add_plan(6.0, move |context| {
            context.remove_itinerary_modifier::<Alive>();
            assert_eq!(context.get_dominant_itinerary_modifier(p1), Some(school_closure_modifier));
        });

        context.add_plan(8.0, move |context| {
            context.remove_itinerary_modifier::<Age>();
            assert_eq!(context.get_dominant_itinerary_modifier(p1), None);
        });
        context.execute();    
    }
}
