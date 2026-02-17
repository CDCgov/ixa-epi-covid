use crate::{
    population_loader::PersonId,};

use indexmap::set::IndexSet;

use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, HashMapExt, IxaError, PluginContext, define_entity, define_rng, impl_property
};
use serde::{Deserialize, Serialize};


define_entity!(Setting);

define_rng!(SettingsRng);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub enum SettingCategory {
    Home,
    Workplace,
    School,
    CensusTract,
}

impl_property!(
    SettingCategory,
    Setting
);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct SettingProperties {
    pub alpha: f64,
}

impl_property!(
    SettingProperties,
    Setting
);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct GeographyProperties {
    pub fips_code: f64,
}

impl_property!(
    GeographyProperties,
    Setting
);


#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct DefaultItineraryProperties {
    pub ratio: f64,
}

impl_property!(
    DefaultItineraryProperties,
    Setting
);

#[derive(Clone, Debug)]
pub struct ItineraryEntry {
    pub setting: SettingId,
    ratio: f64,
}

impl ItineraryEntry {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(setting: SettingId, ratio: f64) -> ItineraryEntry {
        ItineraryEntry {
            setting,
            ratio,
        }
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
    context.get_property::<Setting, DefaultItineraryProperties>(*setting).ratio
}

trait ContextSettingExt: PluginContext + ContextRandomExt {
    fn get_setting_members(
        &self,
        setting: SettingId,
    ) -> Option<&IndexSet<PersonId>> {
        self.query_result_iterator::<Person, _>(())
    }

    fn sample_setting_members_internal(&self, setting: &dyn AnySettingId) -> Option<PersonId> {
        if let Some(members) = self
            .get_data(SettingDataPlugin)
            .get_setting_members(setting)
        {
            if members.is_empty() {
                return None;
            }
            let person = members[self.sample_range(SettingsRng, 0..members.len())];
            return Some(person);
        }
        None
    }

    fn get_itinerary_internal(&self, person_id: PersonId) -> Option<&Vec<ItineraryEntry>> {
        self.get_data(SettingDataPlugin).get_itinerary(person_id)
    }
}