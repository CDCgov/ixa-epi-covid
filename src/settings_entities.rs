use crate::{
    population_loader::{Person, PersonCensusTractId, PersonHomeId, PersonId, PersonSchoolId, PersonWorkplaceId},
    setting_loader::{CensusTract, Home, School, SettingCategory, SettingEntityProperties, Workplace},
};

use ixa::{entity::{EntitySetIterator}, prelude::*};

define_rng!(SettingsRng);
// #[allow(dead_code)]
// #[derive(Clone, Debug)]
// pub struct ItineraryEntry {
//     pub setting: SettingId,
//     ratio: f64,
// }

// impl ItineraryEntry {
//     #[allow(clippy::needless_pass_by_value)]
//     pub fn new(setting: SettingId, ratio: f64) -> ItineraryEntry {
//         ItineraryEntry { setting, ratio }
//     }
// }

// pub fn append_itinerary_entry(
//     itinerary: &mut Vec<ItineraryEntry>,
//     context: &Context,
//     setting: SettingId,
//     nondefault_ratio: Option<f64>,
// ) -> Result<(), IxaError> {
//     let ratio = match nondefault_ratio {
//         Some(user_input) => user_input,
//         None => get_default_itinerary_ratio(context, &setting),
//     };
//     itinerary.push(ItineraryEntry::new(setting, ratio));
//     Ok(())
// }

fn calculate(
        property: SettingEntityProperties,
        members_count: usize
    ) -> f64 {
        if members_count == 0 {
            0.0
        } else {
            property.alpha * (members_count - 1) as f64
        }
    }

pub trait ContextSettingExt: PluginContext + ContextRandomExt {
    fn get_setting_members(&'_ self, setting: SettingCategory) -> EntitySetIterator<'_, Person> {
        match setting {
            SettingCategory::Home(home_id) => self.query_result_iterator::<Person, _>((PersonHomeId(home_id),)),
            SettingCategory::Workplace(workplace_id) => {
                self.query_result_iterator::<Person, _>((PersonWorkplaceId(Some(workplace_id)),))
            }
            SettingCategory::School(school_id) => {
                self.query_result_iterator::<Person, _>((PersonSchoolId(Some(school_id)),))
            }
            SettingCategory::CensusTract(census_tract_id) => {
                self.query_result_iterator::<Person, _>((PersonCensusTractId(census_tract_id),))
            }
        }
    }

    fn sample_setting_member(&mut self, setting: SettingCategory) -> Option<PersonId> {
        match setting {
            SettingCategory::Home(home_id) => {
                self.sample_entity::<Person, _, _>(SettingsRng, (PersonHomeId(home_id),))
            }
            SettingCategory::Workplace(workplace_id) => {
                self.sample_entity::<Person, _, _>(SettingsRng, (PersonWorkplaceId(Some(workplace_id)),))
            }
            SettingCategory::School(school_id) => {
                self.sample_entity::<Person, _, _>(SettingsRng, (PersonSchoolId(Some(school_id)),))
            }
            SettingCategory::CensusTract(census_tract_id) => {
                self.sample_entity::<Person, _, _>(SettingsRng, (PersonCensusTractId(census_tract_id),))
            }
        }
    }

    fn sample_setting_member_excluding(
        &mut self,
        setting: SettingCategory,
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

    fn get_settings(&self, person_id: PersonId) -> Vec<SettingCategory> {
        let home_id = self.get_property::<Person, PersonHomeId>(person_id).0;
        let school_id = self.get_property::<Person, PersonSchoolId>(person_id).0;
        let workplace_id = self.get_property::<Person, PersonWorkplaceId>(person_id).0;
        let census_tract_id = self.get_property::<Person, PersonCensusTractId>(person_id).0;
        let mut setting_options = vec![SettingCategory::Home(home_id), SettingCategory::CensusTract(census_tract_id)];
        if let Some(school) = school_id {
            setting_options.push(SettingCategory::School(school));
        }
        if let Some(workplace) = workplace_id {
            setting_options.push(SettingCategory::Workplace(workplace));
        }
        setting_options
    }

    fn calculate_multiplier(
        &self, 
        setting: SettingCategory
    ) -> f64 {
        match setting {
            SettingCategory::Home(setting_id) => {
                let props = self.get_property::<Home, SettingEntityProperties>(setting_id);
                let member_count = self.get_setting_members(setting).count();
                calculate(props, member_count)
            },
            SettingCategory::Workplace(setting_id) => {
                let props = self.get_property::<Workplace, SettingEntityProperties>(setting_id);
                let member_count = self.get_setting_members(setting).count();
                calculate(props, member_count)
            },
            SettingCategory::School(setting_id) => {
                let props = self.get_property::<School, SettingEntityProperties>(setting_id);
                let member_count = self.get_setting_members(setting).count();
                calculate(props, member_count)
            },
            SettingCategory::CensusTract(setting_id) => {
                let props = self.get_property::<CensusTract, SettingEntityProperties>(setting_id);
                let member_count = self.get_setting_members(setting).count();
                calculate(props, member_count)
            },
        }
    }

    fn sample_setting(&mut self, person_id: PersonId) -> SettingCategory {
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
