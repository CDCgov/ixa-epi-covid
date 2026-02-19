use crate::{
    population_loader::{CensusTractId, HomeId, Person, PersonId, SchoolId, WorkplaceId},
};

use ixa::{entity::EntitySetIterator, prelude::*};
use serde::{Deserialize, Serialize};

define_rng!(SettingsRng);

define_entity!(Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct SettingCode (pub usize);
impl_property!(SettingCode, Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct StateCode(pub usize);
impl_property!(StateCode, Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct CountyCode(pub usize);
impl_property!(CountyCode, Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct CensusTractCode(pub usize);
impl_property!(CensusTractCode, Setting);
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum SettingCategory {
    Home,
    Workplace,
    School,
    CensusTract,
}

impl_property!(SettingCategory, Setting);

define_property!(struct Alpha(pub f64), Setting);

define_property!(struct DefaultItineraryRatio(pub f64), Setting);

pub fn get_default_itinerary_ratio(context: &Context, setting: &SettingId) -> f64 {
    context
        .get_property::<Setting, DefaultItineraryRatio>(*setting)
        .0
}

pub trait ContextSettingExt: PluginContext + ContextRandomExt {
    fn get_setting_members(&'_ self, setting: SettingId) -> EntitySetIterator<'_, Person> {
        let setting_category = self.get_property::<Setting, SettingCategory>(setting);
        match setting_category {
            SettingCategory::Home => self.query_result_iterator::<Person, _>((HomeId(setting),)),
            SettingCategory::Workplace => {
                self.query_result_iterator::<Person, _>((WorkplaceId(Some(setting)),))
            }
            SettingCategory::School => {
                self.query_result_iterator::<Person, _>((SchoolId(Some(setting)),))
            }
            SettingCategory::CensusTract => {
                self.query_result_iterator::<Person, _>((CensusTractId(setting),))
            }
        }
    }

    fn sample_setting_member(&mut self, setting: SettingId) -> Option<PersonId> {
        let setting_category = self.get_property::<Setting, SettingCategory>(setting);
        match setting_category {
            SettingCategory::Home => {
                self.sample_entity::<Person, _, _>(SettingsRng, (HomeId(setting),))
            }
            SettingCategory::Workplace => {
                self.sample_entity::<Person, _, _>(SettingsRng, (WorkplaceId(Some(setting)),))
            }
            SettingCategory::School => {
                self.sample_entity::<Person, _, _>(SettingsRng, (SchoolId(Some(setting)),))
            }
            SettingCategory::CensusTract => {
                self.sample_entity::<Person, _, _>(SettingsRng, (CensusTractId(setting),))
            }
        }
    }

    fn sample_setting_member_excluding(
        &mut self,
        setting: SettingId,
        exclude: &[PersonId],
    ) -> Option<PersonId> {
        if self.get_setting_members(setting).count() <= exclude.len() {
            return None;
        }
        loop {
            let sampled = self.sample_setting_member(setting)?;
            if !exclude.contains(&sampled) {
                return Some(sampled);
            }
        }
    }

    fn get_settings(&self, person_id: PersonId) -> Vec<SettingId> {
        let home_id = self.get_property::<Person, HomeId>(person_id).0;
        let school_id = self.get_property::<Person, SchoolId>(person_id).0;
        let workplace_id = self.get_property::<Person, WorkplaceId>(person_id).0;
        let census_tract_id = self.get_property::<Person, CensusTractId>(person_id).0;
        let mut setting_options = vec![home_id, census_tract_id];
        if let Some(school) = school_id {
            setting_options.push(school);
        }
        if let Some(workplace) = workplace_id {
            setting_options.push(workplace);
        }
        setting_options
    }

    fn calculate_multiplier(
        &self, 
        setting: SettingId
    ) -> f64 {
        let alpha = self.get_property::<Setting, Alpha>(setting);
        let members = self.get_setting_members(setting).collect::<Vec<_>>();
        if members.is_empty() {
            0.0
        } else {
            alpha.0 * (members.len() - 1) as f64
        }
    }

    fn sample_setting(&mut self, person_id: PersonId) -> SettingId {
        let settings = self.get_settings(person_id);
        settings[self.sample_range(SettingsRng, 0..settings.len())]
    }

    /// Get the total current infectiousness multiplier for a person
    /// This is the sum of the infectiousness multipliers for each setting derived from the itinerary
    /// with members filtered as Active and in the Current itinerary
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of active members in the setting
    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let settings = self.get_settings(person_id);
        let mut collector = 0.0;
        for setting in &settings{
            collector += self.calculate_multiplier(*setting);
        }
        collector
    }
    /// Get the maximum infectiousness multiplier for a person across all settings
    /// derived from both the default and modified itineraries of the person.
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of all active and inactive members in the setting
    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let settings  = self.get_settings(person_id);
        let mut collector = 0.0;
        for setting in &settings {
            let multiplier = self.calculate_multiplier(*setting);
            collector = f64::max(collector, multiplier);
        };
        collector
    }
}
impl ContextSettingExt for Context {}
