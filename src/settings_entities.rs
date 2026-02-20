use crate::{
    population_loader::{PersonId},
    itinerary::{ContextItineraryExt},
};

use ixa::{prelude::*};
use serde::{Deserialize, Serialize};

define_rng!(SettingsRng);

define_entity!(Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct SettingCode (pub usize);
impl_property!(SettingCode, Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct StateCode(pub usize);
impl_property!(StateCode, Setting);

// #[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
// pub struct CountyCode(pub usize);
// impl_property!(CountyCode, Setting);

// #[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
// pub struct CensusTractCode(pub usize);
// impl_property!(CensusTractCode, Setting);

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


pub trait ContextSettingExt: PluginContext + ContextRandomExt + ContextItineraryExt {
    fn sample_setting_member_excluding(
        &mut self,
        setting: SettingId,
        exclude: &[PersonId],
    ) -> Option<PersonId> {
        if self.get_setting_members(setting).len() <= exclude.len() {
            return None;
        }
        loop {
            let sampled = self.sample_setting_member(setting)?;
            if !exclude.contains(&sampled) {
                return Some(sampled);
            }
        }
    }

    fn calculate_multiplier(
        &self, 
        setting: SettingId
    ) -> f64 {
        let alpha = self.get_property::<Setting, Alpha>(setting);
        let members = self.get_setting_members(setting);
        if members.is_empty() {
            0.0
        } else {
            alpha.0 * (members.len() - 1) as f64
        }
    }

    
    fn sample_setting(&mut self, person_id: PersonId) -> SettingId {
        let settings = self.get_active_settings_for_person(person_id);
        let weights: Vec<f64> = settings.iter().map(|(_, w)| *w).collect();
        let total: f64 = weights.iter().sum();
        
        if total == 0.0 {
            return settings[self.sample_range(SettingsRng, 0..settings.len())].0;
        }
        
        let mut val = self.sample_range(SettingsRng, 0.0..total);
        weights.iter().enumerate()
            .find(|(_, w)| { val -= *w; val <= 0.0 })
            .map(|(i, _)| settings[i].0)
            .unwrap_or_else(|| settings[settings.len() - 1].0)
    }

    /// Get the total current infectiousness multiplier for a person
    /// This is the sum of the infectiousness multipliers for each setting derived from the itinerary
    /// with members filtered as Active and in the Current itinerary
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of active members in the setting
    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let settings = self.get_active_settings_for_person(person_id);
        let mut collector = 0.0;
        for setting in &settings{
            collector += setting.1 * self.calculate_multiplier(setting.0);
        }
        collector
    }
    /// Get the maximum infectiousness multiplier for a person across all settings
    /// derived from both the default and modified itineraries of the person.
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of all active and inactive members in the setting
    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let settings  = self.get_all_settings_for_person(person_id);
        let mut collector = 0.0;
        for setting in &settings {
            let multiplier = self.calculate_multiplier(setting.0);
            collector = f64::max(collector, multiplier);
        };
        collector
    }
}
impl ContextSettingExt for Context {}
