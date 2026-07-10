use ixa::prelude::*;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::{
    population_loader::{Person, PersonId},
    settings::{Itinerary, SETTING_COUNT},
};

use dyn_clone::DynClone;

pub trait ItineraryModifierTrait: std::fmt::Debug + DynClone + 'static {
    fn layer(&mut self, other: Box<dyn ItineraryModifierTrait>) -> Box<dyn ItineraryModifierTrait>;
    fn apply(&mut self, base_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT];
    fn as_any(&self) -> &dyn Any;
}

// This is implemented for testing
impl PartialEq for dyn ItineraryModifierTrait {
    fn eq(&self, other: &Self) -> bool {
        self.as_any().type_id() == other.as_any().type_id()
    }
}

pub trait ItineraryModifierStorageTrait: std::fmt::Debug + Any {
    fn get_itinerary_modifiers(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<Vec<Box<dyn ItineraryModifierTrait>>>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

type PersonPropertyItineraryModifier<'a, P> = (
    P,
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    HashMap<P, Vec<Box<dyn ItineraryModifierTrait>>>,
);

impl<P> ItineraryModifierStorageTrait for PersonPropertyItineraryModifier<'static, P>
where
    P: Property<Person> + std::fmt::Debug + std::hash::Hash + Eq,
{
    fn get_itinerary_modifiers(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Option<Vec<Box<dyn ItineraryModifierTrait>>> {
        let (_person_property, modifier_map) = self;
        let property_val = context.get_property::<Person, P>(person_id);
        modifier_map
            .get(&property_val)
            .map(|v| v.iter().map(|b| dyn_clone::clone_box(b.as_ref())).collect())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
struct ItineraryModifierContainer {
    itinerary_modifier_map: HashMap<TypeId, Box<dyn ItineraryModifierStorageTrait>>,
}

define_data_plugin!(
    ItineraryModifierPlugin,
    ItineraryModifierContainer,
    ItineraryModifierContainer::default()
);

pub trait ContextItineraryModifierExt: PluginContext + ContextEntitiesExt {
    /// Register a generic itinerary modifier.
    fn register_itinerary_modifier<
        P: Property<Person> + std::fmt::Debug + std::hash::Hash + Eq + 'static,
        I: ItineraryModifierTrait,
    >(
        &mut self,
        person_property: P,
        itinerary_modifier: I,
    ) 
    {
        if let Some(modifier_map) = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .get_mut(&TypeId::of::<P>())
        {
            if let Some(downcast_modifier_map) = modifier_map
                .as_any_mut()
                .downcast_mut::<PersonPropertyItineraryModifier<P>>(
            ) {
                let (_property, itinerary_modifier_map) = downcast_modifier_map;
                itinerary_modifier_map
                    .entry(person_property)
                    .or_insert_with(Vec::new)
                    .push(Box::new(itinerary_modifier));
            }
        } else {
            let person_property_modifier: PersonPropertyItineraryModifier<P> = (
                person_property,
                HashMap::from_iter([(
                    person_property,
                    Vec::from([Box::new(itinerary_modifier) as Box<dyn ItineraryModifierTrait>]),
                )]),
            );
            // Insert the boxed modifier into the itinerary modifier map
            let _ = self
                .get_data_mut(ItineraryModifierPlugin)
                .itinerary_modifier_map
                .insert(TypeId::of::<P>(), Box::new(person_property_modifier));
        }
    }

    fn remove_itinerary_modifier_by_property<P: Property<Person> + std::hash::Hash + Eq +'static>(
        &mut self,
        property_value: P,
    )
    {
        let modifier_map = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .get_mut(&TypeId::of::<P>());
        if let Some(property_modifier_map) = modifier_map
            && let Some(downcast_property_modifier_map) = property_modifier_map
                .as_any_mut()
                .downcast_mut::<PersonPropertyItineraryModifier<P>>(
            )
        {
            let (_property, itinerary_modifier_map) = downcast_property_modifier_map;
            itinerary_modifier_map.remove(&property_value);
        }
    }

    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<Box<dyn ItineraryModifierTrait>>;
    fn get_itinerary(&self, person_id: PersonId) -> [f64; SETTING_COUNT];
}
impl ContextItineraryModifierExt for Context {
    // This needs to be here to have access to the concrete context type for the get_itinerary trait method
    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<Box<dyn ItineraryModifierTrait>> {
        let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
        let mut modifiers: Vec<Box<dyn ItineraryModifierTrait>> = Vec::new();
        for modifier in itinerary_modifier_container.itinerary_modifier_map.values() {
            let itinerary_modifier_vec = modifier.get_itinerary_modifiers(self, person_id);
            if let Some(itinerary_modifier_vec) = itinerary_modifier_vec {
                modifiers.extend(itinerary_modifier_vec);
            }
        }
        modifiers
    }

    fn get_itinerary(&self, person_id: PersonId) -> [f64; SETTING_COUNT] {
        let base_itinerary = self
            .get_property::<Person, Itinerary>(person_id)
            .itinerary_ratios;
        let modifiers = self.get_itinerary_modifiers(person_id);
        let mut layered_modifier: Option<Box<dyn ItineraryModifierTrait>> = None;
        for modifier in modifiers {
            layered_modifier = Some(match layered_modifier {
                Some(mut existing) => existing.layer(modifier),
                None => modifier,
            });
        }
        if let Some(mut layered_modifier) = layered_modifier {
            layered_modifier.apply(&base_itinerary)
        } else {
            base_itinerary
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Age;
    use crate::itinerary_modifiers::define_itinerary_modifier;
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::settings::{Itinerary, SettingCategory};
    use ixa::HashMap;

    fn setup() -> Context {
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
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Work, 0.25),
                (SettingCategory::Community, 0.25),
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
        let mut context = setup();
        let weekend_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];
        let weekend_modifier = define_itinerary_modifier(Some(weekend_transient_matrix), None);

        context.register_itinerary_modifier(Age(11), weekend_modifier);
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(11))).unwrap();
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p1.len(), 0);
        assert_eq!(
            modifiers_p2,
            vec![Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>]
        );

        context.register_itinerary_modifier(Age(10), weekend_modifier);
        assert_eq!(
            context.get_itinerary_modifiers(p1),
            vec![Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>]
        );
        assert_eq!(
            context.get_itinerary_modifiers(p2),
            vec![Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>]
        );
    }

    #[test]
    fn test_register_multiple_itinerary_modifiers() {
        let mut context = setup();

        let weekend_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let weekend_modifier = define_itinerary_modifier(Some(weekend_transient_matrix), None);

        let school_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.75, 0.0, 0.0, 0.25],
            [0.75, 0.0, 0.0, 0.25],
            [0.75, 0.0, 0.0, 0.25],
        ];

        let school_modifier = define_itinerary_modifier(Some(school_transient_matrix), None);

        context.register_itinerary_modifier(Age(11), weekend_modifier);
        context.register_itinerary_modifier(Age(11), school_modifier);
        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        let modifiers = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers.len(), 2);
        assert!(
            modifiers.contains(&(Box::new(school_modifier) as Box<dyn ItineraryModifierTrait>))
        );
        assert!(
            modifiers.contains(&(Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>))
        );
    }

    #[test]
    fn test_itinerary_modifier_removal_by_property() {
        let mut context = setup();
        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let weekend_modifier = define_itinerary_modifier(Some(weekend_matrix), None);
        context.register_itinerary_modifier(Age(10), weekend_modifier);
        context.register_itinerary_modifier(Age(11), weekend_modifier);
        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers_p1.len(), 1);
        assert!(
            modifiers_p1.contains(&(Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>))
        );

        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p2.len(), 1);
        assert!(
            modifiers_p2.contains(&(Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>))
        );

        // This would remove all age based itinerary modifiers that is not ideal.
        context.remove_itinerary_modifier_by_property::<Age>(Age(11));
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers_p1.len(), 0);

        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p2.len(), 1);
        assert!(
            modifiers_p2.contains(&(Box::new(weekend_modifier) as Box<dyn ItineraryModifierTrait>))
        );
    }

    #[test]
    fn test_shelter_in_place_and_weekends() {
        // We have a shelter in place + a weekend. Shelter in place moves the time spent doing 50%
        // of community activities to doing home activities. It also moves work and school activities to home
        //  Weekends change time spent doing school/work         // activities to doing activities in the home or community. We will assume that time is split equally
        // between home and the community.
        // If your initial itinerary was Home (0.3) Work (0.0) School (0.5) Com (0.2).
        // Under shelter in place your itinerary would be Home (0.9) Work (0.0) School (0.0) Com (0.1)
        // Under weekend your itinerary would be Home (0.55) Work (0.0) School (0.0) Com (0.45)
        // Under both shelter in place and weekend Home (0.775) Work (0.0) School (0.0) Com (0.225)
        // The solution for comparison is worked by hand

        let mut context = setup();
        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let weekend_modifier = define_itinerary_modifier(Some(weekend_matrix), None);

        let sip_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.0],
        ];

        let sip_location_matrix = [
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let sip_modifier =
            define_itinerary_modifier(Some(sip_transient_matrix), Some(sip_location_matrix));

        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();

        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, None, None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.register_itinerary_modifier(Age(11), weekend_modifier);

        let modified_itinerary = context.get_itinerary(p1);
        assert_eq!(modified_itinerary, [0.55, 0.0, 0.0, 0.45]);

        context.register_itinerary_modifier(Age(11), sip_modifier);

        let modified_itinerary = context.get_itinerary(p1);
        assert_eq!(modified_itinerary, [0.775, 0.0, 0.0, 0.225]);
    }

    #[test]
    fn test_isolation_and_weekends() {
        // We have isolation + weekend. Isolation changes the location of all activities
        // to home. Weekends change time spent doing school/work activities to doing activities
        // in the home or community. We will assume that time is split equally between home and
        // the community.
        // If your initial itinerary was Home (0.25) Work (0.25) School (0.25) Com (0.25).
        // Under isolation your itinerary would be Home (1) Work (0.0) School (0.0) Com (0.0)
        // Under weekend your itinerary would be Home (0.5) Work (0.0) School (0.0) Com (0.5)
        // Under both isolation and weekend Home (1) Work (0.0) School (0.0) Com (0.0)
        // THe solution for comparison is worked by hand

        let mut context = setup();
        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let weekend_modifier = define_itinerary_modifier(Some(weekend_matrix), None);
        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        context.register_itinerary_modifier(Age(11), weekend_modifier);
        context.register_itinerary_modifier(Age(11), isolation_modifier);
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, None, None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        let modified_itinerary = context.get_itinerary(p1);
        assert_eq!(modified_itinerary, [1.0, 0.0, 0.0, 0.0]);
    }
}
