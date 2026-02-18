use crate::{
    population_loader::{CensusTractId, HomeId, Person, PersonId, SchoolId, WorkplaceId},
    setting_loader::{DefaultItineraryProperties, Setting, SettingCategory, SettingId},
};

use ixa::{entity::EntitySetIterator, prelude::*};

define_rng!(SettingsRng);
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ItineraryEntry {
    pub setting: SettingId,
    ratio: f64,
}

impl ItineraryEntry {
    #[allow(clippy::needless_pass_by_value)]
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
    // Is this setting type registered? Our population loader is hard coded to always try to put
    // people in the core setting types, but sometimes we don't want all the core setting types
    // (we didn't specify them). So, first check that the setting in question exists.

    let ratio = match nondefault_ratio {
        Some(user_input) => user_input,
        None => get_default_itinerary_ratio(context, &setting),
    };
    itinerary.push(ItineraryEntry::new(setting, ratio));
    Ok(())
}

fn get_default_itinerary_ratio(context: &Context, setting: &SettingId) -> f64 {
    context
        .get_property::<Setting, DefaultItineraryProperties>(*setting)
        .ratio
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
            SettingCategory::Home => self.sample_entity::<Person, _, _>(
            SettingsRng,
            (HomeId(setting),)
            ),
            SettingCategory::Workplace => self.sample_entity::<Person, _, _>(
            SettingsRng,
            (WorkplaceId(Some(setting)),)
            ),
            SettingCategory::School => self.sample_entity::<Person, _, _>(
            SettingsRng,
            (SchoolId(Some(setting)),)
            ),
            SettingCategory::CensusTract => self.sample_entity::<Person, _, _>(
            SettingsRng,
            (CensusTractId(setting),)
            ),
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

}
impl ContextSettingExt for Context {}
