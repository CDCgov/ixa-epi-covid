use ixa::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ContextParametersExt, Params, population_loader::PersonId};

define_rng!(SettingRng);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub usize);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct Alpha(pub f64);

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq)]
pub enum SettingCategory {
    Home,
    School,
    Work,
    Community,
}

define_entity!(Setting);
impl_property!(SettingCategory, Setting);
impl_property!(SettingCode, Setting);
impl_property!(Alpha, Setting);

define_entity!(PersonSetting);

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub struct PersonRef(pub PersonId);
impl_property!(PersonRef, PersonSetting);

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub struct SettingRef(pub SettingId);
impl_property!(SettingRef, PersonSetting);

fn index_setting_tables(context: &mut Context) {
    context.index_property::<Setting, SettingCategory>();
    context.index_property::<Setting, SettingCode>();
    context.index_property::<PersonSetting, PersonRef>();
    context.index_property::<PersonSetting, SettingRef>();
}

pub trait ContextSettingExt:
    PluginContext + ContextEntitiesExt + ContextRandomExt + ContextParametersExt
{
    fn get_setting_category(&self, setting: SettingId) -> SettingCategory {
        self.get_property::<Setting, SettingCategory>(setting)
    }

    fn get_setting_alpha(&self, setting: SettingId) -> Result<f64, IxaError> {
        Ok(self.get_property::<Setting, Alpha>(setting).0)
    }

    fn get_setting_ratio(&self, setting: SettingId) -> Result<f64, IxaError> {
        let Params {
            itinerary_ratios, ..
        } = self.get_params();
        let kind = self.get_setting_category(setting);
        Ok(*itinerary_ratios.get(&kind).unwrap())
    }

    fn get_setting_size(&self, setting: SettingId) -> Result<usize, IxaError> {
        Ok(self.query_entity_count::<PersonSetting, _>((SettingRef(setting),)))
    }

    fn sample_person_from_setting(&self, setting: SettingId) -> Result<PersonId, IxaError> {
        if let Some(membership) =
            self.sample_entity::<PersonSetting, _, _>(SettingRng, (SettingRef(setting),))
        {
            return Ok(self.get_property::<PersonSetting, PersonRef>(membership).0);
        }

        Err(IxaError::IxaError(format!(
            "No members found for setting id: {:?}",
            setting
        )))
    }

    fn get_active_settings_for_person(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<SettingId>, IxaError> {
        let mut settings = Vec::new();
        for membership in self.query_result_iterator::<PersonSetting, _>((PersonRef(person_id),)) {
            settings.push(self.get_property::<PersonSetting, SettingRef>(membership).0);
        }
        Ok(settings)
    }

    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut ratios = Vec::new();
        let mut multipliers = Vec::new();
        for setting_id in active_settings.iter() {
            let multiplier = self.calculate_multipler(*setting_id).unwrap();
            let ratio = self.get_setting_ratio(*setting_id).unwrap();
            ratios.push(ratio);
            multipliers.push(multiplier);
        }
        let sum_ratios: f64 = ratios.iter().sum();
        let mut current_inf = 0.0;
        if sum_ratios > 0.0 {
            for (multiplier, ratio) in multipliers.iter().zip(ratios.iter()) {
                current_inf += multiplier * (ratio / sum_ratios);
            }
        }
        current_inf
    }

    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut max_inf = 0.0;
        for setting_id in active_settings.iter() {
            let multiplier = self.calculate_multipler(*setting_id).unwrap();
            max_inf = f64::max(max_inf, multiplier);
        }
        max_inf
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: PersonId,
        setting: SettingId,
    ) -> Result<Option<PersonId>, IxaError> {
        if self.get_setting_size(setting)? == 1 {
            return Ok(None);
        }
        loop {
            let sampled_person = self.sample_person_from_setting(setting)?;
            if sampled_person != person_id {
                return Ok(Some(sampled_person));
            }
        }
    }

    fn calculate_multipler(&self, setting: SettingId) -> Result<f64, IxaError> {
        let size = self.get_setting_size(setting)?;
        let alpha = self.get_setting_alpha(setting)?;
        Ok(((size.saturating_sub(1)) as f64).powf(alpha))
    }

    fn sample_active_setting(&self, person_id: PersonId) -> Result<SettingId, IxaError> {
        let mut weights = Vec::new();
        let ids = self.get_active_settings_for_person(person_id)?;
        let mut sum_weights = 0.0;
        for id in ids.iter() {
            let ratio = self.get_setting_ratio(*id)?;
            let multiplier = self.calculate_multipler(*id)?;
            weights.push(multiplier * ratio);
            sum_weights += multiplier * ratio;
        }
        if sum_weights > 0.0 {
            let setting_index = self.sample_weighted(SettingRng, &weights);
            Ok(ids[setting_index])
        } else {
            let setting_index = self.sample_range(SettingRng, 0..ids.len());
            Ok(ids[setting_index])
        }
    }

    fn add_person_to_setting(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
        setting_code: SettingCode,
        alpha: Alpha,
    ) -> Result<(), IxaError> {
        let setting_id = self.add_index_setting(setting_category, setting_code, alpha)?;
        self.add_entity::<PersonSetting, _>((PersonRef(person_id), SettingRef(setting_id)))?;
        Ok(())
    }

    fn add_index_setting(
        &mut self,
        setting_category: SettingCategory,
        setting_code: SettingCode,
        alpha: Alpha,
    ) -> Result<SettingId, IxaError> {
        if let Some(setting_id) = self
            .query_result_iterator::<Setting, _>((setting_category, setting_code))
            .next()
        {
            return Ok(setting_id);
        }
        self.add_entity::<Setting, _>((setting_category, setting_code, alpha))
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    index_setting_tables(context);
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::GlobalParams;
    use crate::population_loader::Person;
    use ixa::{HashMap, assert_almost_eq};

    fn setup() -> Context {
        let mut context = Context::new();
        index_setting_tables(&mut context);
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
        context
    }

    #[test]
    fn test_get_setting_alpha() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::Home, SettingCode(6), Alpha(0.7)))
            .unwrap();
        let a = context.get_setting_alpha(setting_id).unwrap();
        assert_eq!(a, 0.7);
    }

    #[test]
    fn test_get_setting_size() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::Home, SettingCode(5), Alpha(0.5)))
            .unwrap();
        let person1 = context.add_entity::<Person, _>((crate::Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((crate::Age(21),)).unwrap();
        context
            .add_person_to_setting(person1, SettingCategory::Home, SettingCode(5), Alpha(0.5))
            .unwrap();
        context
            .add_person_to_setting(person2, SettingCategory::Home, SettingCode(5), Alpha(0.5))
            .unwrap();
        let size = context.get_setting_size(setting_id).unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_get_setting_ratio() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::School, SettingCode(7), Alpha(0.5)))
            .unwrap();
        let ratio = context.get_setting_ratio(setting_id).unwrap();
        assert_eq!(ratio, 0.25);
    }

    #[test]
    fn test_get_active_settings_for_person() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::Home, SettingCode(7), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((crate::Age(22),)).unwrap();
        context
            .add_person_to_setting(person_id, SettingCategory::Home, SettingCode(7), Alpha(0.5))
            .unwrap();
        let active = context.get_active_settings_for_person(person_id).unwrap();
        assert!(active.contains(&setting_id));
    }

    #[test]
    fn test_calculate_current_infectiousness_multiplier_for_person() {
        let mut context = setup();
        let p1 = context.add_entity::<Person, _>((crate::Age(23),)).unwrap();
        let p2 = context.add_entity::<Person, _>((crate::Age(23),)).unwrap();
        let p3 = context.add_entity::<Person, _>((crate::Age(23),)).unwrap();
        context
            .add_person_to_setting(p1, SettingCategory::Home, SettingCode(8), Alpha(0.5))
            .unwrap();
        context
            .add_person_to_setting(p2, SettingCategory::Home, SettingCode(8), Alpha(0.5))
            .unwrap();
        context
            .add_person_to_setting(p3, SettingCategory::Home, SettingCode(8), Alpha(0.5))
            .unwrap();
        context
            .add_person_to_setting(p1, SettingCategory::Work, SettingCode(9), Alpha(0.5))
            .unwrap();
        context
            .add_person_to_setting(p2, SettingCategory::Work, SettingCode(9), Alpha(0.5))
            .unwrap();

        let val = context.calculate_current_infectiousness_multiplier_for_person(p1);
        assert_almost_eq!(val, 1.207, 0.001);
        // home size = 3, alpha = 0.5 => (3-1)^0.5 = 1.4142
        // work size = 2, alpha = 0.5 => (2-1)^0.5 = 1
        // ratios are equal, so (1.4142/2) + (1/2) ~= 1.207
    }

    #[test]
    fn test_calculate_max_infectiousness_multiplier_for_person() {
        let mut context = setup();
        let p1 = context.add_entity::<Person, _>((crate::Age(24),)).unwrap();
        let p2 = context.add_entity::<Person, _>((crate::Age(24),)).unwrap();
        context
            .add_person_to_setting(p1, SettingCategory::Home, SettingCode(9), Alpha(1.0))
            .unwrap();
        context
            .add_person_to_setting(p2, SettingCategory::Home, SettingCode(9), Alpha(1.0))
            .unwrap();
        context
            .add_person_to_setting(p1, SettingCategory::Work, SettingCode(10), Alpha(1.0))
            .unwrap();
        let val = context.calculate_max_infectiousness_multiplier_for_person(p1);
        assert_eq!(val, 1.0);
    }

    #[test]
    fn test_sample_person_from_setting() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::Community, SettingCode(10), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((crate::Age(25),)).unwrap();
        context
            .add_person_to_setting(
                person_id,
                SettingCategory::Community,
                SettingCode(10),
                Alpha(0.5),
            )
            .unwrap();
        let sampled = context.sample_person_from_setting(setting_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_from_setting_with_exclusion() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::Work, SettingCode(11), Alpha(0.5)))
            .unwrap();
        let p1 = context.add_entity::<Person, _>((crate::Age(26),)).unwrap();
        let p2 = context.add_entity::<Person, _>((crate::Age(27),)).unwrap();
        context
            .add_person_to_setting(p1, SettingCategory::Work, SettingCode(11), Alpha(0.5))
            .unwrap();
        context
            .add_person_to_setting(p2, SettingCategory::Work, SettingCode(11), Alpha(0.5))
            .unwrap();
        let sampled = context
            .sample_from_setting_with_exclusion(p1, setting_id)
            .unwrap();
        assert_eq!(sampled, Some(p2));
    }

    #[test]
    fn test_sample_active_setting() {
        let mut context = setup();
        let setting_id = context
            .add_entity::<Setting, _>((SettingCategory::Home, SettingCode(13), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((crate::Age(30),)).unwrap();
        context
            .add_person_to_setting(
                person_id,
                SettingCategory::Home,
                SettingCode(13),
                Alpha(0.5),
            )
            .unwrap();
        let sampled = context.sample_active_setting(person_id).unwrap();
        assert_eq!(sampled, setting_id);
    }

    #[test]
    fn test_add_person_to_setting_and_add_index_setting() {
        let mut context = setup();
        let person_id = context.add_entity::<Person, _>((crate::Age(31),)).unwrap();
        let setting_code = SettingCode(14);
        let alpha = Alpha(0.3);
        context
            .add_person_to_setting(person_id, SettingCategory::Home, setting_code, alpha)
            .unwrap();
        let setting_id = context
            .query_result_iterator::<Setting, _>((SettingCategory::Home, setting_code))
            .next()
            .unwrap();
        let stored_code = context.get_property::<Setting, SettingCode>(setting_id);
        assert_eq!(stored_code, setting_code);
    }
}
