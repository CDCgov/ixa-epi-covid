use crate::{
    parameters::{ContextParametersExt, CoreSettingsTypes, Params},
    population_loader::PersonId, settings_entities,
};

use indexmap::set::IndexSet;

use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, HashMap, HashMapExt, HashSet, IxaError, PluginContext, define_data_plugin, define_entity, define_rng, impl_property, profiling::open_span
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
pub struct DefaultItineraryProperties {
    pub ratio: f64,
}

impl_property!(
    DefaultItineraryProperties,
    Setting
);

// pub struct SettingCode{
//     pub setting_id: usize,
// }

// impl_property!(
//     SettingCode,
//     Setting
// );

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
            None => get_itinerary_ratio(context, &setting)?,
        };
        itinerary.push(ItineraryEntry::new(setting, ratio));
    }
    Ok(())
}

fn get_itinerary_ratio(context: &Context, setting: &SettingId) -> Result<f64, IxaError> {
    let itinerary_ratio = context.get_property<Setting, DefaultItineraryProperties>(setting);

    match itinerary_ratio {
        Some(ratio) => Ok(*ratio),
        None => Err(IxaError::from("Itinerary ratio not specified")),
    }
}

