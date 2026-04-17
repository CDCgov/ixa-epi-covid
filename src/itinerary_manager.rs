use ixa::{HashMap, prelude::*};
use serde::{Serialize};
use strum::IntoEnumIterator;

use crate::{ContextParametersExt, population_loader::{ItineraryRatios, Person, PersonId, SettingIds}, settings::{ContextSettingExt, SettingCategory}};

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct ItineraryModifier {
    pub ranking: usize,
    pub itinerary_ratios: ItineraryRatios,
}

/// An index of settings as represented by their setting codes.
#[derive(Default)]
pub struct ItineraryModifiers {
    modifiers: HashMap<PersonId, Vec<ItineraryModifier>>,
    dominant_modifier: HashMap<PersonId, ItineraryModifier>,
}

impl ItineraryModifiers {
    pub fn new() -> Self {
        Self {
            modifiers: HashMap::default(),
            dominant_modifier: HashMap::default(),
        }
    }
    fn update_dominant_modifier(&mut self, person: PersonId) {
        if let Some(modifiers) = self.modifiers.get(&person) {
            if let Some(max_modifier) = modifiers.iter().max_by_key(|m| m.ranking) {
                self.dominant_modifier.insert(person, max_modifier.clone());
            }
        }
    }

    pub fn add_itinerary_modifier(&mut self, person: PersonId, itinerary_modifier: ItineraryModifier) {
        let modifiers = self.modifiers.entry(person).or_default();
        if !modifiers.contains(&itinerary_modifier) {
            modifiers.push(itinerary_modifier);
            self.update_dominant_modifier(person);
        }
    }

    pub fn get_modifiers(&self, person: PersonId) -> Option<&Vec<ItineraryModifier>> {
        self.modifiers.get(&person)
    }

    pub fn get_modifiers_mut(&mut self, person: PersonId) -> Option<&mut Vec<ItineraryModifier>> {
        self.modifiers.get_mut(&person)
    }

    pub fn remove_itinerary_modifier(&mut self, person: PersonId, itinerary_modifier: ItineraryModifier) {
        self.modifiers
            .entry(person)
            .and_modify(|modifiers|modifiers.retain(|m| *m != itinerary_modifier));
        self.update_dominant_modifier(person);
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
        itinerary_modifiers.add_itinerary_modifier(person_id, itinerary_modifier);
        itinerary_modifiers.update_dominant_modifier(person_id);
    }
    itinerary_modifiers
});



pub trait ContextItineraryModifierExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt {
    fn register_itinerary_modifier(&mut self, person: PersonId, itinerary_modifier: ItineraryModifier) {
        let container = self.get_data_mut(ItineraryModifiersPlugin);
        container.add_itinerary_modifier(person, itinerary_modifier);
        self.implement_dominant_multiplier(person);
    }

    fn remove_itinerary_modifier(&mut self, person: PersonId, itinerary_modifier: ItineraryModifier) {
        let container = self.get_data_mut(ItineraryModifiersPlugin);
        container.remove_itinerary_modifier(person, itinerary_modifier);
        self.implement_dominant_multiplier(person);
    }

    fn implement_dominant_multiplier(&mut self, person: PersonId) {
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
            let school_id = self.get_property::<Person, SettingIds>(person_id).setting_ids[SettingCategory::School];
            let work_id = self.get_property::<Person, SettingIds>(person_id).setting_ids[SettingCategory::Work];
            let school_closure_work_modifier_start = school_closure_work_modifier.clone();
            let school_closure_work_modifier_end = school_closure_work_modifier.clone();
            let school_closure_modifier_start = school_closure_modifier.clone();
            let school_closure_modifier_end = school_closure_modifier.clone();
            if school_id.is_some() {
                if work_id.is_some() {
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
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.implement_school_closure(1.0, 40.0);
    Ok(())
}
