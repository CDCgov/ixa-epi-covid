use ixa::prelude::*;
use serde::Serialize;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::population_loader::{Person, PersonId};
use crate::settings::ItineraryRatios;

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifier {
    ranking: usize,
    itinerary_ratios: ItineraryRatios,
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

pub trait ItineraryModifierTrait: std::fmt::Debug + Any {
    fn get_itineraries(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<Vec<ItineraryModifier>>;
    fn as_any(&self) -> &dyn Any;
}

type PersonPropertyItineraryModifier<'a, P> = (
    P,
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    HashMap<<P as Property<Person>>::CanonicalValue, Vec<ItineraryModifier>>,
);

impl<P> ItineraryModifierTrait for PersonPropertyItineraryModifier<'static, P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    fn get_itineraries(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<Vec<ItineraryModifier>> {
        let (_person_property, modifier_map) = self;
        let property_val = context.get_property::<Person, P>(person_id);
        modifier_map.get(&property_val.make_canonical()).cloned()
    }

    fn as_any(&self) -> &dyn Any {
        self
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
    ) where
        P::CanonicalValue: std::hash::Hash + Eq,
    {
        if let Some(modifier_map) = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .get(&TypeId::of::<P>())
        {
            if let Some(downcast_modifier_map) = modifier_map
                .as_any()
                .downcast_ref::<PersonPropertyItineraryModifier<P>>(
            ) {
                let mut new_modifier_map = downcast_modifier_map.1.clone();
                new_modifier_map
                    .entry(person_property.make_canonical())
                    .or_insert_with(Vec::new)
                    .push(itinerary_modifier);
                let new_person_property_modifier: PersonPropertyItineraryModifier<P> =
                    (downcast_modifier_map.0, new_modifier_map);
                self.get_data_mut(ItineraryModifierPlugin)
                    .itinerary_modifier_map
                    .insert(TypeId::of::<P>(), Box::new(new_person_property_modifier));
            }
        } else {
            let person_property_modifier: PersonPropertyItineraryModifier<P> = (
                person_property,
                HashMap::from_iter([(
                    person_property.make_canonical(),
                    Vec::from([itinerary_modifier]),
                )]),
            );
            // Insert the boxed modifier into the itinerary modifier map
            let _ = self
                .get_data_mut(ItineraryModifierPlugin)
                .itinerary_modifier_map
                .insert(TypeId::of::<P>(), Box::new(person_property_modifier));
        }
    }

    fn remove_itinerary_modifier_by_property<P: Property<Person> + 'static>(
        &mut self,
        property_value: P::CanonicalValue,
    ) where
        <P as ixa::prelude::Property<Person>>::CanonicalValue: std::hash::Hash + Eq,
    {
        let modifier_map = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .get(&TypeId::of::<P>());
        if let Some(property_modifier_map) = modifier_map
            && let Some(downcast_property_modifier_map) = property_modifier_map
                .as_any()
                .downcast_ref::<PersonPropertyItineraryModifier<P>>(
            )
        {
            let mut new_property_modifier_map = downcast_property_modifier_map.1.clone();
            new_property_modifier_map.remove(&property_value);
            let new_person_property_modifier: PersonPropertyItineraryModifier<P> =
                (downcast_property_modifier_map.0, new_property_modifier_map);
            self.get_data_mut(ItineraryModifierPlugin)
                .itinerary_modifier_map
                .insert(TypeId::of::<P>(), Box::new(new_person_property_modifier));
        }
    }

    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<ItineraryModifier>;
    fn get_modified_itinerary(&self, person_id: PersonId) -> Option<ItineraryModifier>;
}
impl ContextItineraryModifierExt for Context {
    // This needs to be here to have access to the concrete context type for the get_itinerary trait method
    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<ItineraryModifier> {
        let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
        let mut modifiers: Vec<ItineraryModifier> = Vec::new();
        for modifier in itinerary_modifier_container.itinerary_modifier_map.values() {
            let itinerary_modifier_vec = modifier.get_itineraries(self, person_id);
            if let Some(itinerary_modifier_vec) = itinerary_modifier_vec {
                modifiers.extend(itinerary_modifier_vec);
            }
        }
        modifiers
    }

    // This needs to be here to have access to the concrete context type for the get_itinerary trait method
    fn get_modified_itinerary(&self, person_id: PersonId) -> Option<ItineraryModifier> {
        self.get_itinerary_modifiers(person_id).into_iter().max()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Age;
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::settings::SettingCategory;
    use ixa::HashMap;

    fn setup(home_ratio: f64, school_ratio: f64, work_ratio: f64, community_ratio: f64) -> Context {
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
            },
        };
        context.register_itinerary_modifier(Age(11), school_closure_modifier);
        let p1 = context.add_entity::<Person, _>((Age(10),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let dominant_modifier_p1 = context.get_modified_itinerary(p1);
        let dominant_modifier_p2 = context.get_modified_itinerary(p2);
        assert_eq!(dominant_modifier_p1, None);
        assert_eq!(dominant_modifier_p2, Some(school_closure_modifier));

        context.register_itinerary_modifier(Age(10), school_closure_modifier);
        assert_eq!(
            context.get_modified_itinerary(p1),
            Some(school_closure_modifier)
        );
        assert_eq!(
            context.get_modified_itinerary(p2),
            Some(school_closure_modifier)
        );
    }

    #[test]
    fn test_register_multiple_itinerary_modifiers() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
        let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            },
        };
        let work_closure_modifier = ItineraryModifier {
            ranking: 2,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.25, 0.0],
            },
        };
        context.register_itinerary_modifier(Age(11), school_closure_modifier);
        context.register_itinerary_modifier(Age(11), work_closure_modifier);
        let p1 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let modifiers = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers.len(), 2);
        assert!(modifiers.contains(&school_closure_modifier));
        assert!(modifiers.contains(&work_closure_modifier));
    }

    #[test]
    fn test_itinerary_modifier_removal_by_property() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
        let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            },
        };
        context.register_itinerary_modifier(Age(10), school_closure_modifier);
        context.register_itinerary_modifier(Age(11), school_closure_modifier);
        let p1 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(10),)).unwrap();
        assert_eq!(
            context.get_modified_itinerary(p1),
            Some(school_closure_modifier)
        );
        assert_eq!(
            context.get_modified_itinerary(p2),
            Some(school_closure_modifier)
        );
        // This would remove all age based itinerary modifiers that is not ideal.
        context.remove_itinerary_modifier_by_property::<Age>(Age(11));
        assert_eq!(context.get_modified_itinerary(p1), None);
        assert_eq!(
            context.get_modified_itinerary(p2),
            Some(school_closure_modifier)
        );
    }

    #[test]
    fn test_itinerary_modifier_dominance() {
        let mut context = setup(0.25, 0.25, 0.25, 0.25);
        let school_closure_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            },
        };
        let work_closure_modifier = ItineraryModifier {
            ranking: 2,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.25, 0.0],
            },
        };
        context.register_itinerary_modifier(Age(11), school_closure_modifier);
        context.register_itinerary_modifier(Age(11), work_closure_modifier);
        let p1 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        assert_eq!(
            context.get_modified_itinerary(p1),
            Some(work_closure_modifier)
        );
    }
}
