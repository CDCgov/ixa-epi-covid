use crate::{
    ContextParametersExt, Params, itinerary::ContextItineraryExt, population_loader::PersonId,
};

use ixa::prelude::*;
use serde::{Deserialize, Serialize};

define_rng!(SettingsRng);

define_entity!(Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct SettingCode(pub usize);
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

pub trait ContextSettingExt:
    PluginContext + ContextRandomExt + ContextItineraryExt + ContextParametersExt
{
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

    fn calculate_multiplier(&self, setting: SettingId) -> f64 {
        let alpha = self.get_property::<Setting, Alpha>(setting);
        let members = self.get_setting_members(setting);
        if members.is_empty() {
            0.0
        } else {
            ((members.len() - 1) as f64).powf(alpha.0)
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
        weights
            .iter()
            .enumerate()
            .find(|(_, w)| {
                val -= *w;
                val <= 0.0
            })
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
        for setting in &settings {
            collector += setting.1 * self.calculate_multiplier(setting.0);
        }
        collector
    }
    /// Get the maximum infectiousness multiplier for a person across all settings
    /// derived from both the default and modified itineraries of the person.
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of all active and inactive members in the setting
    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let settings = self.get_all_settings_for_person(person_id);
        let mut collector = 0.0;
        for setting in &settings {
            let multiplier = self.calculate_multiplier(setting.0);
            collector = f64::max(collector, multiplier);
        }
        collector
    }

    fn add_setting(
        &mut self,
        setting_category: SettingCategory,
        setting_string: String,
    ) -> Result<SettingId, IxaError> {
        let setting_code: usize = setting_string
            .parse()
            .map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", setting_string)))?;
        if let Some(setting_id) = self
            .query_result_iterator::<Setting, _>((SettingCode(setting_code), setting_category))
            .next()
        {
            return Ok(setting_id);
        }
        let fips = get_fips_from_string(setting_string.clone())?;
        let alpha = *self.get_default_setting_properties(setting_category)?;
        let itinerary_ratio = self.get_default_itinerary_ratio(setting_category)?;
        let setting_id: SettingId = self.add_entity::<Setting, _>((
            SettingCode(setting_code),
            StateCode(fips.0),
            setting_category,
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?;
        Ok(setting_id)
    }

    fn get_default_setting_properties(
        &self,
        setting_category: SettingCategory,
    ) -> Result<&f64, IxaError> {
        let Params {
            settings_properties,
            ..
        } = self.get_params();
        settings_properties.get(&setting_category).ok_or_else(|| {
            IxaError::IxaError(format!(
                "No properties found for setting category: {:?}",
                setting_category
            ))
        })
    }

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
}
impl ContextSettingExt for Context {}

pub fn get_fips_from_string(setting_string: String) -> Result<(usize, usize, usize), IxaError> {
    if setting_string.len() < 11 {
        return Err(IxaError::IxaError(format!(
            "Invalid FIPS code length: {}",
            setting_string
        )));
    }
    let state_code: usize = setting_string[0..2].parse().map_err(|_| {
        IxaError::IxaError(format!("Invalid state code in FIPS: {}", setting_string))
    })?;
    let county_code: usize = setting_string[2..5].parse().map_err(|_| {
        IxaError::IxaError(format!("Invalid county code in FIPS: {}", setting_string))
    })?;
    let tract_code: usize = setting_string[5..11].parse().map_err(|_| {
        IxaError::IxaError(format!("Invalid tract code in FIPS: {}", setting_string))
    })?;
    Ok((state_code, county_code, tract_code))
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::{
//         Age, parameters::GlobalParams, population_loader::PersonId, settings::ContextSettingExt,
//     };
//     use ixa::{ContextEntitiesExt, ContextGlobalPropertiesExt, assert_almost_eq};

//     define_setting_category!(Community);

//     fn register_default_settings(context: &mut Context) {
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         context
//             .register_setting_category(&Workplace, SettingProperties { alpha: 0.3 }, 1.0)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
//             .unwrap();

//         context
//             .register_setting_category(&School, SettingProperties { alpha: 0.01 }, 1.0)
//             .unwrap();
//     }

//     #[test]
//     fn test_setting_category_creation() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 0.5)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.001 }, 0.25)
//             .unwrap();
//         let home_props = context.get_setting_properties(&Home).unwrap();
//         let tract_props = context.get_setting_properties(&CensusTract).unwrap();

//         let home_ratio = context.get_setting_itinerary_ratio(&Home).unwrap();
//         let tract_ratio = context.get_setting_itinerary_ratio(&CensusTract).unwrap();

//         assert_almost_eq!(0.1, home_props.alpha, 0.0);
//         assert_eq!(0.5, home_ratio);
//         assert_almost_eq!(0.001, tract_props.alpha, 0.0);
//         assert_eq!(0.25, tract_ratio);
//     }

//     #[test]
//     fn test_get_properties_after_registration() {
//         let mut context = Context::new();
//         let e = context.get_setting_properties(&Home).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(
//                     msg,
//                     "Attempting to get properties of unregistered setting type"
//                 );
//             }
//             Some(ue) => panic!(
//                 "Expected an error setting plugin data is none. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }

//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         context.get_setting_properties(&Home).unwrap();
//         let e = context.get_setting_properties(&CensusTract).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(
//                     msg,
//                     "Attempting to get properties of unregistered setting type"
//                 );
//             }
//             Some(ue) => panic!(
//                 "Expected an error attempting to get properties of unregistered setting type. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }

//         context.get_setting_itinerary_ratio(&Home).unwrap();
//         let e = context.get_setting_itinerary_ratio(&CensusTract).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(
//                     msg,
//                     "Attempting to get itinerary ratio of unregistered setting type"
//                 );
//             }
//             Some(ue) => panic!(
//                 "Expected an error attempting to get itinerary ratio of unregistered setting type. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_duplicate_setting_category_registration() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         let e = context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.001 }, 1.0)
//             .err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Setting type is already registered");
//             }
//             Some(ue) => panic!(
//                 "Expected an error that there are duplicate settings types. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_duplicated_itinerary() {
//         let mut context = Context::new();
//         register_default_settings(&mut context);

//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary = vec![
//             ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
//             ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
//         ];
//         let e = context.add_itinerary(person, itinerary).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Duplicated setting");
//             }
//             Some(ue) => panic!(
//                 "Expected an error that there are duplicate settings. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_feasible_itinerary_ratio() {
//         let mut context = Context::new();
//         register_default_settings(&mut context);
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 2), -0.5)];

//         let e = context.add_itinerary(person, itinerary).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Setting ratio must be greater than or equal to 0");
//             }
//             Some(ue) => panic!(
//                 "Expected an error setting ratios should be greater than or equal to 0. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_feasible_itinerary_setting() {
//         let mut context = Context::new();
//         register_default_settings(&mut context);
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();

//         // Community is a defined setting but not registered
//         let itinerary = vec![ItineraryEntry::new(SettingId::new(Community, 2), 0.5)];

//         let e = context.add_itinerary(person, itinerary).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Itinerary entry setting type not registered");
//             }
//             Some(ue) => panic!(
//                 "Expected an error setting . Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_change_activity_members() {
//         let mut context = Context::new();
//         register_default_settings(&mut context);
//         let active_person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let inactive_person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let active_itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 1), 1.0)];
//         let inactive_itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 1), 0.0)];
//         context
//             .add_itinerary(active_person, active_itinerary.clone())
//             .unwrap();
//         context
//             .add_itinerary(inactive_person, inactive_itinerary.clone())
//             .unwrap();

//         let home = SettingId::new(Home, 1);

//         let members = context.get_setting_members_internal(&home).unwrap();

//         assert_eq!(members.len(), 1);
//     }

//     #[test]
//     fn test_add_itinerary() {
//         let mut context = Context::new();
//         register_default_settings(&mut context);
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary = vec![
//             ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
//             ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
//         ];
//         context.add_itinerary(person, itinerary).unwrap();
//         let members = context
//             .get_setting_members(&SettingId::new(Home, 2))
//             .unwrap();
//         assert_eq!(members.len(), 1);

//         let person2: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary2 = vec![ItineraryEntry::new(SettingId::new(Home, 2), 1.0)];
//         context.add_itinerary(person2, itinerary2).unwrap();

//         let members2 = context
//             .get_setting_members(&SettingId::new(Home, 2))
//             .unwrap();
//         assert_eq!(members2.len(), 2);

//         let itinerary3 = vec![ItineraryEntry::new(SettingId::new(Home, 3), 0.5)];

//         let e = context.add_itinerary(person, itinerary3).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Person already has an itinerary.");
//             }
//             Some(ue) => panic!(
//                 "Expected an error setting . Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_get_itinerary() {
//         let mut context = Context::new();
//         register_default_settings(&mut context);
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let default_itinerary = vec![
//             ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
//             ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
//         ];
//         context.add_itinerary(person, default_itinerary).unwrap();

//         let default = context.get_itinerary(person).unwrap();

//         for entry in default {
//             assert_almost_eq!(entry.ratio, 0.5, 0.0);
//         }
//     }

//     #[test]
//     fn test_setting_registration() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
//             .unwrap();
//         for s in 0..5 {
//             for _ in 0..5 {
//                 let person: PersonId = context.add_entity((Age(30),)).unwrap();
//                 let itinerary = vec![
//                     ItineraryEntry::new(SettingId::new(Home, s), 0.5),
//                     ItineraryEntry::new(SettingId::new(CensusTract, s), 0.5),
//                 ];
//                 context.add_itinerary(person, itinerary).unwrap();
//             }
//             let members = context
//                 .get_setting_members(&SettingId::new(Home, s))
//                 .unwrap();
//             let tract_members = context
//                 .get_setting_members(&SettingId::new(CensusTract, s))
//                 .unwrap();

//             assert_eq!(members.len(), 5);
//             assert_eq!(tract_members.len(), 5);
//         }
//     }

//     #[test]
//     fn test_setting_registration_activity() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
//             .unwrap();
//         for s in 0..5 {
//             for _ in 0..5 {
//                 let person: PersonId = context.add_entity((Age(30),)).unwrap();
//                 let itinerary = vec![
//                     ItineraryEntry::new(SettingId::new(Home, s), 1.0),
//                     ItineraryEntry::new(SettingId::new(CensusTract, s), 0.0),
//                 ];
//                 context.add_itinerary(person, itinerary).unwrap();
//             }
//             let all_home_members = context
//                 .get_setting_members_internal(&SettingId::new(Home, s))
//                 .unwrap();
//             let all_tract_members =
//                 context.get_setting_members_internal(&SettingId::new(CensusTract, s));

//             assert_eq!(all_home_members.len(), 5);
//             assert_eq!(all_tract_members, None);
//         }
//     }

//     #[test]
//     fn test_setting_multiplier() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         for s in 0..5 {
//             // Create 5 people
//             for _ in 0..5 {
//                 let person: PersonId = context.add_entity((Age(30),)).unwrap();
//                 let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, s), 0.5)];
//                 context.add_itinerary(person, itinerary).unwrap();
//             }
//         }

//         let home_id = 0;
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, home_id), 0.5)];
//         context.add_itinerary(person, itinerary).unwrap();
//         let members = context
//             .get_setting_members(&SettingId::new(Home, home_id))
//             .unwrap();

//         let setting_type = &SettingId::new(Home, home_id);

//         let inf_multiplier =
//             setting_type.calculate_multiplier(members, SettingProperties { alpha: 0.1 });

//         // This is assuming we know what the function for Home is (N - 1) ^ alpha
//         assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);
//     }

//     #[test]
//     fn test_total_infectiousness_multiplier() {
//         // Go through all the settings and compute infectiousness multiplier
//         let mut context = Context::new();
//         register_default_settings(&mut context);

//         for s in 0..5 {
//             for _ in 0..5 {
//                 let person: PersonId = context.add_entity((Age(30),)).unwrap();
//                 let itinerary = vec![
//                     ItineraryEntry::new(SettingId::new(Home, s), 0.5),
//                     ItineraryEntry::new(SettingId::new(CensusTract, s), 0.5),
//                 ];
//                 context.add_itinerary(person, itinerary).unwrap();
//             }
//         }
//         // Create a new person and register to home 0
//         let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         context.add_itinerary(person, itinerary).unwrap();

//         // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
//         let inf_multiplier = context.calculate_max_infectiousness_multiplier_for_person(person);
//         assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);

//         // If person's itinerary is changed for two settings,
//         // CensusTract 0 should have 6 members, Home 0 should have 7 members
//         // the total infectiousness should be the sum of infs * proportion
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary_complete = vec![
//             ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
//             ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
//         ];
//         context.add_itinerary(person, itinerary_complete).unwrap();
//         let members_home = context
//             .get_setting_members(&SettingId::new(Home, 0))
//             .unwrap();
//         let members_tract = context
//             .get_setting_members(&SettingId::new(CensusTract, 0))
//             .unwrap();
//         assert_eq!(members_home.len(), 7);
//         assert_eq!(members_tract.len(), 6);

//         let inf_multiplier_two_settings =
//             context.calculate_max_infectiousness_multiplier_for_person(person);

//         let alpha_h = context.get_setting_properties(&Home).unwrap().alpha;
//         let alpha_ct = context.get_setting_properties(&CensusTract).unwrap().alpha;

//         assert_almost_eq!(
//             inf_multiplier_two_settings,
//             f64::max(
//                 f64::from(7 - 1).powf(alpha_h),
//                 f64::from(6 - 1).powf(alpha_ct)
//             ),
//             0.0
//         );
//     }

//     #[test]
//     fn test_sample_setting() {
//         let mut context = Context::new();
//         context.init_random(42);
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
//             .unwrap();

//         let person_a: PersonId = context.add_entity((Age(30),)).unwrap();
//         let person_b: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary_a = vec![
//             ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
//             ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
//         ];
//         let itinerary_b = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
//         context.add_itinerary(person_a, itinerary_a).unwrap();
//         context.add_itinerary(person_b, itinerary_b).unwrap();

//         // When person a is used to select a setting for contact, it should return Home. While they are
//         // also a member of CensusTract, since they are the only member the multiplier used to weight the
//         // selection is 0.0 from calculate_multiplier. Thus the probability CensusTract is selected is 0.0.
//         let setting_id = context.sample_current_setting(person_a).unwrap();
//         assert_eq!(setting_id.get_type_id(), TypeId::of::<Home>());
//         assert_eq!(setting_id.id(), 0);

//         let setting_id = context.sample_current_setting(person_b).unwrap();
//         assert_eq!(setting_id.get_type_id(), TypeId::of::<Home>());
//         assert_eq!(setting_id.id(), 0);

//         let person_c: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary_c = vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5)];
//         context.add_itinerary(person_c, itinerary_c).unwrap();
//         let setting_id = context.sample_current_setting(person_c).unwrap();
//         assert_eq!(setting_id.get_type_id(), TypeId::of::<CensusTract>());
//         assert_eq!(setting_id.id(), 0);
//     }

//     #[test]
//     fn test_get_contact_from_setting() {
//         // Register two people to a setting and make sure that the person chosen is the other one
//         // Attempt to draw a contact from a setting with only the person trying to get a contact
//         let mut context = Context::new();
//         context.init_random(42);
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
//             .unwrap();

//         let person_a: PersonId = context.add_entity((Age(30),)).unwrap();
//         let person_b: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary_a = vec![
//             ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
//             ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
//         ];
//         let itinerary_b = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
//         context.add_itinerary(person_a, itinerary_a).unwrap();
//         context.add_itinerary(person_b, itinerary_b).unwrap();
//         let setting_id = context.sample_current_setting(person_a).unwrap();
//         let members = context.get_setting_members(setting_id).unwrap();
//         assert!(members.contains(&person_a));

//         assert_eq!(
//             Some(person_b),
//             context
//                 .sample_from_setting_with_exclusion(person_a, setting_id)
//                 .unwrap()
//         );

//         assert!(
//             context
//                 .sample_from_setting_with_exclusion(person_a, &SettingId::new(CensusTract, 0))
//                 .unwrap()
//                 .is_none()
//         );

//         let person_c: PersonId = context.add_entity((Age(30),)).unwrap();
//         let itinerary_c = vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5)];
//         context.add_itinerary(person_c, itinerary_c).unwrap();

//         let e =
//             context.sample_from_setting_with_exclusion(person_a, &SettingId::new(CensusTract, 10));
//         match e {
//             Err(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Group membership is None");
//             }
//             Err(ue) => panic!(
//                 "Expected an error attempting contact outside group membership. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             Ok(_) => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_default_itinerary_ratio_specification() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
//             .unwrap();
//         let e = get_itinerary_ratio(&context, &SettingId::new(CensusTract, 0)).err();
//         match e {
//             Some(IxaError::IxaError(msg)) => {
//                 assert_eq!(msg, "Itinerary ratio not specified");
//             }
//             Some(ue) => panic!(
//                 "Expected an error that itinerary specification is not specified. Instead got: {:?}",
//                 ue.to_string()
//             ),
//             None => panic!("Expected an error. Instead, validation passed with no errors."),
//         }
//     }

//     #[test]
//     fn test_append_itinerary_entry() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 0.5)
//             .unwrap();
//         context
//             .register_setting_category(&School, SettingProperties { alpha: 0.2 }, 0.25)
//             .unwrap();
//         let mut itinerary = vec![];

//         // Test appending a valid entry
//         append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 1), None).unwrap();
//         assert_eq!(itinerary.len(), 1);
//         assert_eq!(itinerary[0].setting.get_type_id(), TypeId::of::<Home>());
//         assert_eq!(itinerary[0].setting.id(), 1);
//         assert_almost_eq!(itinerary[0].ratio, 0.5, 0.0);

//         // Test appending an entry with a different setting type
//         append_itinerary_entry(&mut itinerary, &context, SettingId::new(School, 42), None).unwrap();
//         assert_eq!(itinerary.len(), 2);
//         assert_eq!(itinerary[1].setting.get_type_id(), TypeId::of::<School>());
//         assert_eq!(itinerary[1].setting.id(), 42);
//         assert_almost_eq!(itinerary[1].ratio, 0.25, 0.0);

//         // Test appending an entry with a non-default ratio
//         append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 2), Some(1.0))
//             .unwrap();
//         assert_eq!(itinerary.len(), 3);
//         assert_eq!(itinerary[2].setting.get_type_id(), TypeId::of::<Home>());
//         assert_eq!(itinerary[2].setting.id(), 2);
//         assert_almost_eq!(itinerary[2].ratio, 1.0, 0.0);
//     }

//     #[test]
//     fn test_get_itinerary_ratio() {
//         let mut context = Context::new();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 0.5)
//             .unwrap();

//         // Test with a valid setting type
//         let ratio = get_itinerary_ratio(&context, &SettingId::new(Home, 0)).unwrap();
//         assert_almost_eq!(ratio, 0.5, 0.0);
//     }

//     #[test]
//     fn test_only_include_registered_settings_in_itineraries() {
//         let mut context = Context::new();
//         let parameters = Params {
//             settings_properties: HashMap::from_iter(
//                 [(SettingCategory::Home, SettingProperties { alpha: 0.5 })]
//                     .into_iter()
//                     .collect::<HashMap<_, _>>(),
//             ),
//             itinerary_ratios: HashMap::from_iter(
//                 [(SettingCategory::Home, 0.5)]
//                     .into_iter()
//                     .collect::<HashMap<_, _>>(),
//             ),
//             ..Default::default()
//         };

//         context
//             .set_global_property_value(GlobalParams, parameters)
//             .unwrap();

//         init(&mut context);
//         let mut iitinerary = vec![];
//         append_itinerary_entry(
//             &mut iitinerary,
//             &context,
//             SettingId::new(Workplace, 1),
//             None,
//         )
//         .unwrap();

//         assert_eq!(iitinerary.len(), 0);

//         append_itinerary_entry(&mut iitinerary, &context, SettingId::new(Home, 1), None).unwrap();
//         assert_eq!(iitinerary.len(), 1);
//         assert_eq!(iitinerary[0].setting.get_type_id(), TypeId::of::<Home>());
//     }

//     #[test]
//     fn test_itinerary_normalized_one() {
//         let mut context = Context::new();
//         let person: PersonId = context.add_entity((Age(30),)).unwrap();
//         context
//             .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 5.0)
//             .unwrap();
//         context
//             .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 2.5)
//             .unwrap();
//         context
//             .register_setting_category(&School, SettingProperties { alpha: 0.2 }, 2.5)
//             .unwrap();

//         // Test creating an itinerary with all settings
//         let mut itinerary = vec![];
//         append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 1), None).unwrap();
//         append_itinerary_entry(
//             &mut itinerary,
//             &context,
//             SettingId::new(CensusTract, 1),
//             None,
//         )
//         .unwrap();
//         append_itinerary_entry(&mut itinerary, &context, SettingId::new(School, 1), None).unwrap();

//         context.add_itinerary(person, itinerary).unwrap();
//         let itinerary = context.get_itinerary(person).unwrap();

//         let total_ratio: Vec<f64> = itinerary.iter().map(|entry| entry.ratio).collect();
//         assert_eq!(total_ratio, vec![0.5, 0.25, 0.25]);
//     }
// }
