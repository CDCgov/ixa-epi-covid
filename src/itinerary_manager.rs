use ixa::prelude::*;
use serde::Serialize;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::settings::SETTING_COUNT;
use crate::{
    population_loader::{Person, PersonId},
    settings::ItineraryRatios,
};

const TRANSIENT_STATE_COUNT: usize = SETTING_COUNT * 2;

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryTransitionMatrix {
    activity_matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    location_matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    absorption_probabilities: Option<[[f64; TRANSIENT_STATE_COUNT]; SETTING_COUNT]>,
}

#[allow(dead_code)]
#[allow(clippy::needless_range_loop)]
impl ItineraryTransitionMatrix {
    pub fn normalize(&mut self) {
        let normalize_matrix = |matrix: &mut [[f64; SETTING_COUNT]; SETTING_COUNT]| {
            for i in 0..SETTING_COUNT {
                let row_sum: f64 = matrix[i].iter().sum();
                if row_sum > 1.0 {
                    for j in 0..SETTING_COUNT {
                        matrix[i][j] /= row_sum;
                    }
                }
            }
        };

        normalize_matrix(&mut self.activity_matrix);
        normalize_matrix(&mut self.location_matrix);
    }

    pub fn build_transient_and_absorbing_matrix(&self) -> ([[f64; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT], [[f64; TRANSIENT_STATE_COUNT]; SETTING_COUNT]) {
        let mut transient_matrix = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        let mut absorbing_matrix = [[0.0; TRANSIENT_STATE_COUNT]; SETTING_COUNT];
        let activity_row_sums : Vec<f64> = self.activity_matrix.iter().map(|row| row.iter().sum()).collect();
        for i in 0..SETTING_COUNT {
            for j in 0..SETTING_COUNT {
                transient_matrix[i][j] =
                    self.activity_matrix[i][j];
                transient_matrix[i][j + SETTING_COUNT] =
                    1.0 - activity_row_sums[i];
                transient_matrix[i + SETTING_COUNT][j + SETTING_COUNT] =
                    self.location_matrix[i][j];
            }
        }
        let transient_row_sums : Vec<f64> = transient_matrix.iter().map(|row| row.iter().sum()).collect();
        for i in 0..SETTING_COUNT {
            absorbing_matrix[i + SETTING_COUNT][i] = 1.0 - transient_row_sums[i + SETTING_COUNT];
        }
        (transient_matrix, absorbing_matrix)
    }

    pub fn layer(
        &self,
        itinerary_transition_matrix: &ItineraryTransitionMatrix,
    ) -> ItineraryTransitionMatrix {
        let mut layered_activity_matrix = [[0.0; SETTING_COUNT]; SETTING_COUNT];
        let mut layered_location_matrix = [[0.0; SETTING_COUNT]; SETTING_COUNT];

        for i in 0..SETTING_COUNT {
            for j in 0..SETTING_COUNT {
                layered_activity_matrix[i][j] =
                    self.activity_matrix[i][j] + itinerary_transition_matrix.activity_matrix[i][j];
                layered_location_matrix[i][j] =
                    self.location_matrix[i][j] + itinerary_transition_matrix.location_matrix[i][j];
            }
        }

        ItineraryTransitionMatrix {
            activity_matrix: layered_activity_matrix,
            location_matrix: layered_location_matrix,
            absorption_probabilities: None,
        }
    }

    fn calculate_absorption_probabilities(&mut self) {
        // Aborbing probabilities are calculated using the
        // formula N * R, where N is the fundamental matrix (I - Q)^-1, I is the identity matrix,
        // Q is the transient matrix, and R is the absorbing matrix.

        let (transient_matrix, absorbing_matrix) = self.build_transient_and_absorbing_matrix();

        // Create identity matrix I
        let mut identity = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            identity[i][i] = 1.0;
        }

        // Calculate I - Q (where Q is the transient matrix)
        let mut i_minus_q = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            for j in 0..TRANSIENT_STATE_COUNT {
                i_minus_q[i][j] = identity[i][j] - transient_matrix[i][j];
            }
        }

        // Invert (I - Q) to get the fundamental matrix N
        // Using Gaussian elimination with partial pivoting
        let n = i_minus_q;

        // Create augmented identity matrix for inversion
        let mut aug = [[0.0; 2 * TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            for j in 0..TRANSIENT_STATE_COUNT {
                aug[i][j] = n[i][j];
                aug[i][j + TRANSIENT_STATE_COUNT] = if i == j { 1.0 } else { 0.0 };
            }
        }

        // Forward elimination with partial pivoting
        for col in 0..TRANSIENT_STATE_COUNT {
            // Find pivot
            let mut pivot_row = col;
            let mut max_val = aug[col][col].abs();
            for row in col + 1..TRANSIENT_STATE_COUNT {
                if aug[row][col].abs() > max_val {
                    max_val = aug[row][col].abs();
                    pivot_row = row;
                }
            }

            // Swap rows
            if pivot_row != col {
                aug.swap(col, pivot_row);
            }

            // Scale pivot row
            let pivot = aug[col][col];
            if pivot.abs() > f64::EPSILON {
                for j in 0..2 * TRANSIENT_STATE_COUNT {
                    aug[col][j] /= pivot;
                }

                // Eliminate column
                for row in 0..TRANSIENT_STATE_COUNT {
                    if row != col {
                        let factor = aug[row][col];
                        for j in 0..2 * TRANSIENT_STATE_COUNT {
                            aug[row][j] -= factor * aug[col][j];
                        }
                    }
                }
            }
        }

        // Extract inverted matrix and calculate absorption probabilities
        let mut absorption_probs = [[0.0; SETTING_COUNT * 2]; SETTING_COUNT];
        for i in 0..SETTING_COUNT * 2 {
            for j in 0..SETTING_COUNT {
                for k in 0..SETTING_COUNT * 2 {
                    absorption_probs[i][j] += aug[i][k + SETTING_COUNT] * absorbing_matrix[k][j];
                }
            }
        }

        self.absorption_probabilities = Some(absorption_probs);
    }

    pub fn apply(&mut self, current_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT] {
        if self.absorption_probabilities.is_none() {
            self.calculate_absorption_probabilities();
        }
        let absorption_probs = self.absorption_probabilities.unwrap();
        let mut new_itinerary = [0.0; SETTING_COUNT];
        for j in 0..SETTING_COUNT {
            for i in 0..SETTING_COUNT {
                new_itinerary[j] += current_itinerary[i] * absorption_probs[i][j];
            }
        }
        new_itinerary
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct LocationTransitionMatrix {
    matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
}
#[allow(clippy::needless_range_loop)]
impl LocationTransitionMatrix {
    pub fn apply(&self, current_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT] {
        let mut new_itinerary = [0.0; SETTING_COUNT];
        for j in 0..SETTING_COUNT {
            for i in 0..SETTING_COUNT {
                new_itinerary[j] += current_itinerary[i] * self.matrix[i][j];
            }
        }
        new_itinerary
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub enum ItineraryActivity {
    Activity(ActivityTransitionMatrix),
    Location(LocationTransitionMatrix),
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifier {
    modifier_activity: ItineraryActivity,
}

pub trait ItineraryModifierTrait: std::fmt::Debug + Any {
    fn get_itinerary_modifiers(
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
    fn get_itinerary_modifiers(
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
    fn get_modified_itinerary(&self, person_id: PersonId) -> [f64; SETTING_COUNT];
}
impl ContextItineraryModifierExt for Context {
    // This needs to be here to have access to the concrete context type for the get_itinerary trait method
    fn get_itinerary_modifiers(&self, person_id: PersonId) -> Vec<ItineraryModifier> {
        let itinerary_modifier_container = self.get_data(ItineraryModifierPlugin);
        let mut modifiers: Vec<ItineraryModifier> = Vec::new();
        for modifier in itinerary_modifier_container.itinerary_modifier_map.values() {
            let itinerary_modifier_vec = modifier.get_itinerary_modifiers(self, person_id);
            if let Some(itinerary_modifier_vec) = itinerary_modifier_vec {
                modifiers.extend(itinerary_modifier_vec);
            }
        }
        modifiers
    }

    fn get_modified_itinerary(&self, person_id: PersonId) -> [f64; SETTING_COUNT] {
        let base_itinerary = self.get_property::<Person, ItineraryRatios>(person_id);
        let modifiers = self.get_itinerary_modifiers(person_id);
        let activity_modifiers: Vec<ItineraryModifier> = modifiers
            .iter()
            .filter(|modifier| matches!(modifier.modifier_activity, ItineraryActivity::Activity(_)))
            .cloned()
            .collect();
        let location_modifiers: Vec<ItineraryModifier> = modifiers
            .into_iter()
            .filter(|modifier| matches!(modifier.modifier_activity, ItineraryActivity::Location(_)))
            .collect();

        let mut layered_activity_matrix: Option<ActivityTransitionMatrix> = None;
        for modifier in activity_modifiers {
            if let ItineraryActivity::Activity(activity_matrix) = modifier.modifier_activity {
                layered_activity_matrix = Some(match layered_activity_matrix {
                    Some(existing) => existing.layer(&activity_matrix),
                    None => activity_matrix,
                });
            }
        }

        let mut modified_itinerary = [0.0; SETTING_COUNT];
        if let Some(mut layered_activity_matrix) = layered_activity_matrix {
            layered_activity_matrix.normalize();
            layered_activity_matrix.calculate_absorption_probabilities();
            modified_itinerary = layered_activity_matrix.apply(&base_itinerary.itinerary_ratios);
        }
        println!(
            "Itinerary after applying activity modifiers: {:?}",
            modified_itinerary
        );
        for modifier in location_modifiers {
            if let ItineraryActivity::Location(location_matrix) = modifier.modifier_activity {
                modified_itinerary = location_matrix.apply(&modified_itinerary);
            }
        }
        modified_itinerary
    }
}

// need method to check communitivity of location modifier matrices

#[cfg(test)]
mod test {
    use super::*;
    use crate::Age;
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::settings::SettingCategory;
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

        let weekend_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: weekend_transient_matrix,
            absorption_probabilities: None,
        };

        let weekend_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(weekend_activity_transition_matrix),
        };

        context.register_itinerary_modifier(Age(11), weekend_modifier);
        let p1 = context.add_entity::<Person, _>((Age(10),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p1.len(), 0);
        assert_eq!(modifiers_p2, vec![weekend_modifier]);

        context.register_itinerary_modifier(Age(10), weekend_modifier);
        assert_eq!(context.get_itinerary_modifiers(p1), vec![weekend_modifier]);
        assert_eq!(context.get_itinerary_modifiers(p2), vec![weekend_modifier]);
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

        let weekend_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: weekend_transient_matrix,
            absorption_probabilities: None,
        };

        let weekend_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(weekend_activity_transition_matrix),
        };

        let school_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.75, 0.0, 0.0, 0.25],
            [0.75, 0.0, 0.0, 0.25],
            [0.75, 0.0, 0.0, 0.25],
        ];

        let school_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: school_transient_matrix,
            absorption_probabilities: None,
        };

        let school_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(school_activity_transition_matrix),
        };

        context.register_itinerary_modifier(Age(11), weekend_modifier);
        context.register_itinerary_modifier(Age(11), school_modifier);
        let p1 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let modifiers = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers.len(), 2);
        assert!(modifiers.contains(&school_modifier));
        assert!(modifiers.contains(&weekend_modifier));
    }

    #[test]
    fn test_itinerary_modifier_removal_by_property() {
        let mut context = setup();
        let weekend_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let weekend_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: weekend_transient_matrix,
            absorption_probabilities: None,
        };

        let weekend_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(weekend_activity_transition_matrix),
        };
        context.register_itinerary_modifier(Age(10), weekend_modifier);
        context.register_itinerary_modifier(Age(11), weekend_modifier);
        let p1 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(10),)).unwrap();
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers_p1.len(), 1);
        assert!(modifiers_p1.contains(&weekend_modifier));

        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p2.len(), 1);
        assert!(modifiers_p2.contains(&weekend_modifier));

        // This would remove all age based itinerary modifiers that is not ideal.
        context.remove_itinerary_modifier_by_property::<Age>(Age(11));
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        assert_eq!(modifiers_p1.len(), 0);

        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p2.len(), 1);
        assert!(modifiers_p2.contains(&weekend_modifier));
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
        let weekend_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

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
        let weekend_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: weekend_transient_matrix,
            absorption_probabilities: None,
        };

        let sip_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: sip_transient_matrix,
            absorption_probabilities: None,
        };

        let sip_location_transition_matrix = LocationTransitionMatrix {
            matrix: sip_location_matrix,
        };

        let weekend_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(weekend_activity_transition_matrix),
        };

        let sip_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(sip_activity_transition_matrix),
        };

        let itinerary_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Location(sip_location_transition_matrix),
        };
        context.register_itinerary_modifier(Age(11), weekend_modifier);
        let p1 = context.add_entity::<Person, _>((Age(10),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        let modifiers_p1 = context.get_itinerary_modifiers(p1);
        let modifiers_p2 = context.get_itinerary_modifiers(p2);
        assert_eq!(modifiers_p1.len(), 0);
        assert_eq!(modifiers_p2, vec![weekend_modifier]);

        context.register_itinerary_modifier(Age(10), weekend_modifier);
        assert_eq!(context.get_itinerary_modifiers(p1), vec![weekend_modifier]);
        assert_eq!(context.get_itinerary_modifiers(p2), vec![weekend_modifier]);

        context.register_itinerary_modifier(Age(11), sip_modifier);
        context.register_itinerary_modifier(Age(11), itinerary_modifier);
        context.set_property(
            p2,
            ItineraryRatios {
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
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
        let weekend_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let isolation_transient_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let weekend_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: weekend_transient_matrix,
            absorption_probabilities: None,
        };

        let isolation_activity_transition_matrix = ActivityTransitionMatrix {
            transient_matrix: isolation_transient_matrix,
            absorption_probabilities: None,
        };

        let weekend_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(weekend_activity_transition_matrix),
        };

        let isolation_modifier = ItineraryModifier {
            modifier_activity: ItineraryActivity::Activity(isolation_activity_transition_matrix),
        };
        let p1 = context.add_entity::<Person, _>((Age(11),)).unwrap();
        context.register_itinerary_modifier(Age(11), weekend_modifier);
        context.register_itinerary_modifier(Age(11), isolation_modifier);
        context.set_property(
            p1,
            ItineraryRatios {
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        println!(
            "Modifiers for p1: {:?}",
            context.get_itinerary_modifiers(p1)
        );
        println!(
            "Base itinerary for p1: {:?}",
            context.get_property::<Person, ItineraryRatios>(p1)
        );
        println!(
            "Modified itinerary for p1: {:?}",
            context.get_modified_itinerary(p1)
        );
    }
}
