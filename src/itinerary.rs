use ixa::{HashMap, hashing::IndexSet, prelude::*, profiling::open_span};

use crate::{
    ContextParametersExt, Params,
    population_loader::PersonId,
    settings_entities::{Setting, SettingCategory, SettingId},
};

define_rng!(ItineraryRng);

define_data_plugin!(
    ItineraryDataPlugin,
    ItineraryDataContainer,
    ItineraryDataContainer::default()
);

#[derive(Clone, Debug)]
pub struct ItineraryEntry {
    pub setting: SettingId,
    pub ratio: f64,
}

impl ItineraryEntry {
    pub fn new(setting: SettingId, ratio: f64) -> ItineraryEntry {
        ItineraryEntry { setting, ratio }
    }
}

pub fn append_itinerary_entry(
    itinerary: &mut Vec<ItineraryEntry>,
    context: &Context,
    setting: SettingId,
    nondefault_ratio: Option<f64>,
) -> Result<(), IxaError> {
    let ratio = match nondefault_ratio {
        Some(user_input) => user_input,
        None => {
            let setting_category = context.get_property::<Setting, SettingCategory>(setting);
            context.get_default_itinerary_ratio(setting_category)?
        }
    };
    itinerary.push(ItineraryEntry::new(setting, ratio));
    Ok(())
}

#[derive(Default)]
struct ItineraryDataContainer {
    // For each setting type, have a map of each setting id and a list of members
    all_members: HashMap<SettingId, IndexSet<PersonId>>,
    active_itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
    inactive_itineraries: HashMap<PersonId, Vec<ItineraryEntry>>,
}

impl ItineraryDataContainer {
    fn get_setting_members(&self, setting: SettingId) -> Option<&IndexSet<PersonId>> {
        self.all_members.get(&setting)
    }
    fn get_itinerary(&self, person_id: PersonId) -> Vec<ItineraryEntry> {
        self.active_itineraries.get(&person_id).unwrap().clone()
    }
    fn get_all_itineraries(&self, person_id: PersonId) -> Vec<ItineraryEntry> {
        let active = self
            .active_itineraries
            .get(&person_id)
            .into_iter()
            .flatten();
        let inactive = self
            .inactive_itineraries
            .get(&person_id)
            .into_iter()
            .flatten();
        active.chain(inactive).cloned().collect()
    }

    fn activate_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: &Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        let _span = open_span("activate itinerary");
        self.active_itineraries.insert(person_id, itinerary.clone());
        for itinerary_entry in itinerary {
            self.set_member(person_id, itinerary_entry.ratio, itinerary_entry.setting);
        }
        Ok(())
    }
    fn set_member(&mut self, person_id: PersonId, ratio: f64, setting: SettingId) {
        if ratio > 0.0 {
            self.add_member(person_id, setting);
        }
    }

    fn add_member(&mut self, person_id: PersonId, setting: SettingId) {
        self.all_members
            .entry(setting)
            .or_default()
            .insert(person_id);
    }
}

trait ContextItineraryInternalExt: PluginContext {
    fn get_setting_members_internal(&self, setting: SettingId) -> Option<&IndexSet<PersonId>> {
        self.get_data(ItineraryDataPlugin)
            .get_setting_members(setting)
    }

    fn get_active_itinerary_internal(&self, person_id: PersonId) -> Vec<ItineraryEntry> {
        self.get_data(ItineraryDataPlugin).get_itinerary(person_id)
    }

    fn get_all_itineraries_internal(&self, person_id: PersonId) -> Vec<ItineraryEntry> {
        self.get_data(ItineraryDataPlugin)
            .get_all_itineraries(person_id)
    }

    fn add_itinerary_internal(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        self.get_data_mut(ItineraryDataPlugin)
            .activate_itinerary(person_id, &itinerary)
    }
}
impl ContextItineraryInternalExt for Context {}

#[allow(private_bounds)]
pub trait ContextItineraryExt:
    PluginContext + ContextRandomExt + ContextParametersExt + ContextItineraryInternalExt
{
    fn get_default_itinerary_ratio(
        &self,
        setting_category: SettingCategory,
    ) -> Result<f64, IxaError> {
        let Params {
            itinerary_ratios, ..
        } = self.get_params();
        itinerary_ratios
            .get(&setting_category)
            .cloned()
            .ok_or_else(|| {
                IxaError::IxaError(format!(
                    "No itinerary ratio found for setting category: {:?}",
                    setting_category
                ))
            })
    }

    fn add_itinerary(
        &mut self,
        person_id: PersonId,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        let normalied_itinerary = self.normalize_itinerary_ratios(itinerary);
        self.add_itinerary_internal(person_id, normalied_itinerary)
    }

    fn get_itinerary(&'_ self, person_id: PersonId) -> Vec<ItineraryEntry> {
        self.get_active_itinerary_internal(person_id)
    }

    fn get_all_itineraries(&self, person_id: PersonId) -> Vec<ItineraryEntry> {
        self.get_all_itineraries_internal(person_id)
    }

    fn get_setting_members(&self, setting: SettingId) -> Vec<PersonId> {
        self.get_setting_members_internal(setting)
            .map(|set| set.iter().cloned().collect())
            .unwrap()
    }

    fn sample_setting_member(&mut self, setting: SettingId) -> PersonId {
        let members = self.get_setting_members(setting);
        let random_index = self.sample_range(ItineraryRng, 0..members.len());
        members[random_index]
    }

    fn normalize_itinerary_ratios(
        &mut self,
        mut itinerary: Vec<ItineraryEntry>,
    ) -> Vec<ItineraryEntry> {
        let total_ratio: f64 = itinerary.iter().map(|entry| entry.ratio).sum();
        if total_ratio > 0.0 {
            for entry in itinerary.iter_mut() {
                entry.ratio /= total_ratio;
            }
        }
        itinerary
    }
}
impl ContextItineraryExt for Context {}
