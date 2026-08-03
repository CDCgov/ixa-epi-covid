use ixa::prelude::*;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::{
    population_loader::{Person, PersonId},
    settings::{Itinerary, SETTING_COUNT},
};

pub trait ItineraryModifier: std::fmt::Debug + 'static {
    fn layer(&self, other: &dyn ItineraryModifier) -> Box<dyn ItineraryModifier>;
    fn apply(&self, base_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT];
    fn as_any(&self) -> &dyn Any;
    fn accept(&self, _context: &Context, _person_id: PersonId) -> bool {
        true
    }
}

// This is implemented for testing
impl PartialEq for dyn ItineraryModifier {
    fn eq(&self, other: &Self) -> bool {
        self.as_any().type_id() == other.as_any().type_id()
    }
}

pub trait PersonPropertyItineraryModifierStorage: std::fmt::Debug + Any {
    fn get_itinerary_modifiers<'a>(
        &'a self,
        context: &Context,
        person_id: PersonId,
    ) -> Vec<&'a dyn ItineraryModifier>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[derive(Debug)]
struct PersonPropertyItineraryModifier<P> {
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    itinerary_modifier_map: HashMap<P, Vec<Box<dyn ItineraryModifier>>>,
}

impl<P> PersonPropertyItineraryModifierStorage for PersonPropertyItineraryModifier<P>
where
    P: IndexableProperty<Person>,
{
    fn get_itinerary_modifiers(
        &self,
        context: &Context,
        person_id: PersonId,
    ) -> Vec<&dyn ItineraryModifier> {
        let property_val = context.get_property::<Person, P>(person_id);

        self.itinerary_modifier_map
            .get(&property_val)
            .iter()
            .flat_map(|modifiers| modifiers.iter().map(Box::as_ref))
            .collect()
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
    itinerary_modifier_map: HashMap<TypeId, Box<dyn PersonPropertyItineraryModifierStorage>>,
}

define_data_plugin!(
    ItineraryModifierPlugin,
    ItineraryModifierContainer,
    ItineraryModifierContainer::default()
);

pub trait ContextItineraryModifierExt: PluginContext + ContextEntitiesExt {
    /// Register a generic itinerary modifier.
    fn register_itinerary_modifier<P: IndexableProperty<Person>, I: ItineraryModifier>(
        &mut self,
        person_property: P,
        itinerary_modifier: I,
    ) {
        let storage = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .entry(TypeId::of::<P>())
            .or_insert_with(|| {
                Box::new(PersonPropertyItineraryModifier::<P> {
                    itinerary_modifier_map: HashMap::new(),
                })
            });

        let modifier_map = storage
            .as_any_mut()
            .downcast_mut::<PersonPropertyItineraryModifier<P>>()
            .expect("itinerary modifier storage has the wrong type");

        modifier_map
            .itinerary_modifier_map
            .entry(person_property)
            .or_default()
            .push(Box::new(itinerary_modifier));
    }

    fn remove_itinerary_modifier_by_property<P: IndexableProperty<Person>>(
        &mut self,
        property_value: P,
    ) -> Option<Vec<Box<dyn ItineraryModifier>>> {
        let modifier_map = self
            .get_data_mut(ItineraryModifierPlugin)
            .itinerary_modifier_map
            .get_mut(&TypeId::of::<P>());
        if let Some(property_modifier_map) = modifier_map {
            let downcast = property_modifier_map
                .as_any_mut()
                .downcast_mut::<PersonPropertyItineraryModifier<P>>()
                .expect("modifier map entry had unexpected type for its TypeId key");
            return downcast.itinerary_modifier_map.remove(&property_value);
        }
        None
    }

    #[must_use]
    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<&dyn ItineraryModifier>;
    #[must_use]
    fn get_itinerary(&self, person_id: PersonId) -> [f64; SETTING_COUNT];
}
impl ContextItineraryModifierExt for Context {
    // This needs to be here to have access to the concrete context type for the get_itinerary trait method
    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<&dyn ItineraryModifier> {
        let container = self.get_data(ItineraryModifierPlugin);

        container
            .itinerary_modifier_map
            .values()
            .flat_map(|modifier_map| modifier_map.get_itinerary_modifiers(self, person_id))
            .collect()
    }

    fn get_itinerary(&self, person_id: PersonId) -> [f64; SETTING_COUNT] {
        let base_itinerary = self
            .get_property::<Person, Itinerary>(person_id)
            .itinerary_ratios;

        let mut modifiers = self
            .get_itinerary_modifiers(person_id)
            .into_iter()
            .filter(|modifier| modifier.accept(self, person_id));

        let Some(first) = modifiers.next() else {
            return base_itinerary;
        };

        let Some(second) = modifiers.next() else {
            return first.apply(&base_itinerary);
        };

        modifiers
            .fold(first.layer(second), |layered, modifier| {
                layered.layer(modifier)
            })
            .apply(&base_itinerary)
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::Age;
    use crate::itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, assert_same_matrix,
        create_itinerary_transition_matrix,
    };
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::settings::{Itinerary, SettingCategory};
    use ixa::{ExecutionPhase, HashMap};

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

    fn cast_modifier(modifiers: Vec<&dyn ItineraryModifier>) -> Vec<&ItineraryTransitionMatrix> {
        modifiers
            .into_iter()
            .map(|modifier| {
                modifier
                    .as_any()
                    .downcast_ref::<ItineraryTransitionMatrix>()
                    .expect("expected an ItineraryTransitionMatrix")
            })
            .collect()
    }

    #[test]
    fn test_itinerary_modifier_registration() {
        let mut context = setup();
        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let make_weekend_modifier =
            || create_itinerary_transition_matrix(Some(weekend_matrix), None, None);

        let expected_weekend_modifier = make_weekend_modifier();

        context.register_itinerary_modifier(Age(11), make_weekend_modifier());

        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(11))).unwrap();

        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        let modifiers_p2: Vec<&dyn ItineraryModifier> = context.get_itinerary_modifiers(p2);
        let matrices_p2: Vec<&ItineraryTransitionMatrix> = cast_modifier(modifiers_p2.clone());

        assert!(modifiers_p1.is_empty());
        assert_eq!(matrices_p2.len(), 1);
        assert!(assert_same_matrix(
            matrices_p2[0],
            &expected_weekend_modifier
        ));

        context.register_itinerary_modifier(Age(10), make_weekend_modifier());
        let matrices_p1: Vec<&ItineraryTransitionMatrix> =
            cast_modifier(context.get_itinerary_modifiers(p1));
        assert_eq!(matrices_p1.len(), 1);
        assert!(assert_same_matrix(
            matrices_p1[0],
            &expected_weekend_modifier
        ));
        let matrices_p2_after: Vec<&ItineraryTransitionMatrix> =
            cast_modifier(context.get_itinerary_modifiers(p2));
        assert_eq!(matrices_p2_after.len(), 1);
        assert!(assert_same_matrix(
            matrices_p2_after[0],
            &expected_weekend_modifier
        ));
    }

    #[test]
    fn test_register_multiple_itinerary_modifiers() {
        let mut context = setup();

        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let make_weekend_modifier =
            || create_itinerary_transition_matrix(Some(weekend_matrix), None, None);

        let expected_weekend_modifier = make_weekend_modifier();

        let school_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.75, 0.0, 0.0, 0.25],
            [0.75, 0.0, 0.0, 0.25],
            [0.75, 0.0, 0.0, 0.25],
        ];

        let make_school_modifier =
            || create_itinerary_transition_matrix(Some(school_matrix), None, None);

        let expected_school_modifier = make_school_modifier();

        context.register_itinerary_modifier(Age(11), make_weekend_modifier());
        context.register_itinerary_modifier(Age(11), make_school_modifier());
        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        let modifiers: Vec<&dyn ItineraryModifier> = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers.len(), 2);

        let matrices: Vec<&ItineraryTransitionMatrix> = cast_modifier(modifiers);
        assert_eq!(matrices.len(), 2);
        assert!(
            assert_same_matrix(matrices[0], &expected_weekend_modifier)
                || assert_same_matrix(matrices[1], &expected_weekend_modifier)
        );
        assert!(
            assert_same_matrix(matrices[0], &expected_school_modifier)
                || assert_same_matrix(matrices[1], &expected_school_modifier)
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

        let make_weekend_modifier =
            || create_itinerary_transition_matrix(Some(weekend_matrix), None, None);

        let expected_weekend_modifier = make_weekend_modifier();

        context.register_itinerary_modifier(Age(10), make_weekend_modifier());
        context.register_itinerary_modifier(Age(11), make_weekend_modifier());
        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();

        let modifiers_p1: Vec<&dyn ItineraryModifier> = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers_p1.len(), 1);

        let matrices_p1: Vec<&ItineraryTransitionMatrix> = cast_modifier(modifiers_p1);
        assert!(matrices_p1.iter().any(|m| {
            assert_same_matrix(m, &expected_weekend_modifier);
            true
        }));

        let modifiers_p2: Vec<&dyn ItineraryModifier> = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p2.len(), 1);

        let matrices_p2: Vec<&ItineraryTransitionMatrix> = cast_modifier(modifiers_p2);
        assert!(matrices_p2.iter().any(|m| {
            assert_same_matrix(m, &expected_weekend_modifier);
            true
        }));

        // This would remove all age based itinerary modifiers that is not ideal.
        let removed = context.remove_itinerary_modifier_by_property::<Age>(Age(11));
        assert!(removed.is_some());
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers_p1.len(), 0);

        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p2.len(), 1);

        let matrices_p2: Vec<&ItineraryTransitionMatrix> = cast_modifier(modifiers_p2);
        assert!(matrices_p2.iter().any(|m| {
            assert_same_matrix(m, &expected_weekend_modifier);
            true
        }));
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

        let weekend_modifier = create_itinerary_transition_matrix(Some(weekend_matrix), None, None);

        let sip_matrix = [
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
            create_itinerary_transition_matrix(Some(sip_matrix), Some(sip_location_matrix), None);

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

        let weekend_modifier = create_itinerary_transition_matrix(Some(weekend_matrix), None, None);
        let isolation_modifier =
            create_itinerary_transition_matrix(Some(isolation_matrix), None, None);

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

    #[test]
    fn test_single_itinerary_modifier_with_acceptance_function() {
        let mut context = setup();
        let modifier_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];
        let acceptance: AcceptanceFunction =
            Box::new(move |context, _person| context.get_current_time() > 5.0);
        let modifier =
            create_itinerary_transition_matrix(Some(modifier_matrix), None, Some(acceptance));
        context.register_itinerary_modifier(Age(11), modifier);
        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, None, None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        assert_eq!(context.get_itinerary(p1), [0.3, 0.0, 0.5, 0.2]);
        context.add_plan_with_phase(10.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        assert_eq!(context.get_itinerary(p1), [0.55, 0.0, 0.0, 0.45]);
    }

    #[test]
    fn test_two_itinerary_modifiers_with_acceptance_function() {
        let mut context = setup();
        let modifier_one_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ];
        let acceptance: AcceptanceFunction =
            Box::new(move |context, _person| context.get_current_time() > 5.0);
        let modifier_one =
            create_itinerary_transition_matrix(Some(modifier_one_matrix), None, Some(acceptance));
        context.register_itinerary_modifier(Age(11), modifier_one);

        let modifier_two_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 0.0],
        ];
        let acceptance_two: AcceptanceFunction = Box::new(move |context, _person| {
            context.get_current_time() < 10.0 && context.get_current_time() > 7.0
        });
        let modifier_two = create_itinerary_transition_matrix(
            Some(modifier_two_matrix),
            None,
            Some(acceptance_two),
        );
        context.register_itinerary_modifier(Age(11), modifier_two);

        let p1 = context.add_entity(with!(Person, Age(11))).unwrap();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, None, None],
                itinerary_ratios: [0.3, 0.2, 0.4, 0.1],
            },
        );

        assert_eq!(context.get_itinerary(p1), [0.3, 0.2, 0.4, 0.1]);

        context.add_plan(6.0, move |context| {
            assert_eq!(context.get_itinerary(p1), [0.5, 0.0, 0.4, 0.1]);
        });

        context.add_plan(8.0, move |context| {
            assert_eq!(context.get_itinerary(p1), [0.5, 0.0, 0.0, 0.5]);
        });

        context.add_plan_with_phase(20.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        assert_eq!(context.get_itinerary(p1), [0.5, 0.0, 0.4, 0.1]);
    }
}
