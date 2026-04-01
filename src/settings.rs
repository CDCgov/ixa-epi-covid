use ixa::{impl_derived_property, prelude::*};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use core::f64;

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    population_loader::{CommunityId, GenericSetting, HomeId, Person, PersonId, SchoolId, WorkId},
};

define_rng!(SettingRng);

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
}

define_entity!(Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq)]
pub enum SettingCategory {
    Home,
    Work,
    School,
    Community,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub usize);

impl_property!(SettingCode, Setting);

impl_property!(SettingCategory, Setting);

define_multi_property!((SettingCategory, SettingCode), Setting);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq)]
pub struct SettingPropertyIndex(pub usize);

impl_derived_property!(
    SettingPropertyIndex,
    Setting,
    [SettingCategory],
    [],
    |setting_category| match setting_category {
        SettingCategory::Home => SettingPropertyIndex(0),
        SettingCategory::Work => SettingPropertyIndex(1),
        SettingCategory::School => SettingPropertyIndex(2),
        SettingCategory::Community => SettingPropertyIndex(3),
    }
);

define_global_property!(SettingAlphas, [f64; 4]);
define_global_property!(SettingRatios, [f64; 4]);

trait ContextSettingExtPrivate: PluginContext + ContextEntitiesExt + ContextParametersExt {
    fn get_setting_ratio(&self, setting: SettingId) -> Result<f64, ModelError> {
        let ratios = self.get_global_property_value(SettingRatios).unwrap();
        let setting_index = self
            .get_property::<Setting, SettingPropertyIndex>(setting)
            .0;
        Ok(ratios[setting_index])
    }

    fn get_setting_alpha(&self, setting: SettingId) -> Result<f64, ModelError> {
        let alphas = self.get_global_property_value(SettingAlphas).unwrap();
        let setting_index = self
            .get_property::<Setting, SettingPropertyIndex>(setting)
            .0;
        Ok(alphas[setting_index])
    }

    fn sample_person_from_setting_internal<T>(&self, setting: T) -> Result<PersonId, ModelError>
    where
        T: Property<Person> + GenericSetting + Debug,
    {
        self.sample_entity::<Person, _, _>(SettingRng, (setting,))
            .ok_or_else(|| {
                ModelError::ModelError(format!("No members found for setting: {:?}", setting))
            })
    }

    fn get_itinerary_properties_for_person_by_setting<T>(
        &self,
        person_id: PersonId,
    ) -> Result<Option<(SettingId, f64, f64)>, ModelError>
    where
        T: Property<Person> + GenericSetting + Debug,
    {
        if let Some(setting_id) = self.get_property::<Person, T>(person_id).get_setting_id() {
            let ratio = self.get_setting_ratio(setting_id)?;
            let multiplier = self.calculate_multiplier_internal(setting_id)?;
            Ok(Some((setting_id, ratio, multiplier)))
        } else {
            Ok(None)
        }
    }

    fn get_setting_size_internal(&self, setting: SettingId) -> Result<usize, ModelError> {
        let setting_category = self.get_property::<Setting, SettingCategory>(setting);
        match setting_category {
            SettingCategory::Home => {
                Ok(self.query_entity_count::<Person, _>((HomeId(Some(setting)),)))
            }
            SettingCategory::Work => {
                Ok(self.query_entity_count::<Person, _>((WorkId(Some(setting)),)))
            }
            SettingCategory::School => {
                Ok(self.query_entity_count::<Person, _>((SchoolId(Some(setting)),)))
            }
            SettingCategory::Community => {
                Ok(self.query_entity_count::<Person, _>((CommunityId(Some(setting)),)))
            }
        }
    }

    fn calculate_multiplier_internal(&self, setting: SettingId) -> Result<f64, ModelError> {
        let size = self.get_setting_size_internal(setting)? as f64 - 1.0;
        let alpha = self.get_setting_alpha(setting)?;
        Ok(size.powf(alpha))
    }
}
impl ContextSettingExtPrivate for Context {}

#[allow(private_bounds)]
pub trait ContextSettingExt:
    PluginContext + ContextEntitiesExt + ContextSettingExtPrivate + ContextParametersExt
{
    fn register_setting_global_properties(&mut self) {
        let Params {
            settings_properties,
            itinerary_ratios,
            ..
        } = self.get_params().clone();
        self.set_global_property_value(
            SettingAlphas,
            [
                settings_properties
                    .get(&SettingCategory::Home)
                    .unwrap()
                    .alpha,
                settings_properties
                    .get(&SettingCategory::School)
                    .unwrap()
                    .alpha,
                settings_properties
                    .get(&SettingCategory::Work)
                    .unwrap()
                    .alpha,
                settings_properties
                    .get(&SettingCategory::Community)
                    .unwrap()
                    .alpha,
            ],
        )
        .unwrap();
        self.set_global_property_value(
            SettingRatios,
            [
                *itinerary_ratios.get(&SettingCategory::Home).unwrap(),
                *itinerary_ratios.get(&SettingCategory::School).unwrap(),
                *itinerary_ratios.get(&SettingCategory::Work).unwrap(),
                *itinerary_ratios.get(&SettingCategory::Community).unwrap(),
            ],
        )
        .unwrap();
    }

    fn get_setting_size(&self, setting: SettingId) -> Result<usize, ModelError> {
        self.get_setting_size_internal(setting)
    }

    fn get_active_settings_for_person(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<(SettingId, f64, f64)>, ModelError> {
        let mut active_settings = Vec::new();

        if let Some(home) = self
            .get_itinerary_properties_for_person_by_setting::<HomeId>(person_id)
            .unwrap()
        {
            active_settings.push(home);
        }
        if let Some(school) = self
            .get_itinerary_properties_for_person_by_setting::<SchoolId>(person_id)
            .unwrap()
        {
            active_settings.push(school);
        }
        if let Some(work) = self
            .get_itinerary_properties_for_person_by_setting::<WorkId>(person_id)
            .unwrap()
        {
            active_settings.push(work);
        }
        if let Some(community) = self
            .get_itinerary_properties_for_person_by_setting::<CommunityId>(person_id)
            .unwrap()
        {
            active_settings.push(community);
        }
        Ok(active_settings)
    }

    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut current_inf = 0.0;
        let sum_ratios = active_settings.iter().map(|s| s.1).sum::<f64>();
        if sum_ratios > 0.0 {
            current_inf = active_settings
                .iter()
                .map(|s| s.2 * (s.1 / sum_ratios))
                .sum::<f64>();
        }
        current_inf
    }
    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        // this returns the maximum setting specific multipler over the set of active settings
        // for the person
        active_settings.iter().map(|s| s.2).fold(0.0, f64::max)
    }

    fn sample_person_from_setting(&self, setting: SettingId) -> Result<PersonId, ModelError> {
        let setting_category = self.get_property::<Setting, SettingCategory>(setting);
        match setting_category {
            SettingCategory::Home => {
                self.sample_person_from_setting_internal(HomeId(Some(setting)))
            }
            SettingCategory::School => {
                self.sample_person_from_setting_internal(SchoolId(Some(setting)))
            }
            SettingCategory::Work => {
                self.sample_person_from_setting_internal(WorkId(Some(setting)))
            }
            SettingCategory::Community => {
                self.sample_person_from_setting_internal(CommunityId(Some(setting)))
            }
        }
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: PersonId,
        setting: SettingId,
    ) -> Result<Option<PersonId>, ModelError> {
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

    fn calculate_multipler(&self, setting: SettingId) -> Result<f64, ModelError> {
        self.calculate_multiplier_internal(setting)
    }

    fn sample_active_setting(&self, person_id: PersonId) -> Result<SettingId, ModelError> {
        let active_settings = self.get_active_settings_for_person(person_id)?;
        let mut weights_vec = vec![];
        for setting in active_settings.iter() {
            weights_vec.push(setting.1 * setting.2);
        }
        let sum_weights: f64 = weights_vec.iter().sum();
        if sum_weights > 0.0 {
            let setting_index = self.sample_weighted(SettingRng, &weights_vec);
            Ok(active_settings[setting_index].0)
        } else {
            let setting_index = self.sample_range(SettingRng, 0..active_settings.len());
            Ok(active_settings[setting_index].0)
        }
    }

    fn add_person_to_setting(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
        setting_code: SettingCode,
    ) -> Result<(), IxaError> {
        let setting_entity_id = self.add_index_setting(setting_category, setting_code)?;
        match setting_category {
            SettingCategory::Home => {
                self.set_property::<Person, HomeId>(person_id, HomeId(Some(setting_entity_id)))
            }
            SettingCategory::Work => {
                self.set_property::<Person, WorkId>(person_id, WorkId(Some(setting_entity_id)))
            }
            SettingCategory::School => {
                self.set_property::<Person, SchoolId>(person_id, SchoolId(Some(setting_entity_id)))
            }
            SettingCategory::Community => self.set_property::<Person, CommunityId>(
                person_id,
                CommunityId(Some(setting_entity_id)),
            ),
        }
        Ok(())
    }
    fn add_index_setting(
        &mut self,
        setting_category: SettingCategory,
        setting_code: SettingCode,
    ) -> Result<SettingId, IxaError> {
        if let Some(setting_id) = self
            .query_result_iterator::<Setting, _>(((setting_category, setting_code),))
            .next()
        {
            Ok(setting_id)
        } else {
            let setting_id = self
                .add_entity::<Setting, _>((setting_code, setting_category))
                .unwrap();
            Ok(setting_id)
        }
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.index_property::<Setting, SettingCode>();
    context.index_property::<Setting, SettingCategory>();
    context.index_property::<Setting, (SettingCategory, SettingCode)>();
    context.register_setting_global_properties();
    Ok(())
}
// To do: Write tests for each method like the one above in the init
// Write a description with diagram
// Try to convey the issue that we don't have a generic type of entities (trait/generic/hieracrchy)

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Age, Params, parameters::GlobalParams};
    use ixa::{HashMap, assert_almost_eq};

    fn setup(alpha: f64) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha }),
                    (SettingCategory::School, SettingProperties { alpha }),
                    (SettingCategory::Work, SettingProperties { alpha }),
                    (SettingCategory::Community, SettingProperties { alpha }),
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
        init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_get_setting_size_empty() {
        let mut context = setup(0.0);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(100), SettingCategory::Home))
            .unwrap();
        // No persons assigned yet
        let size = context.get_setting_size(home_id).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_get_setting_size_multiple_categories() {
        let mut context = setup(0.0);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(101), SettingCategory::Home))
            .unwrap();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(102), SettingCategory::Work))
            .unwrap();
        let person1 = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        let person3 = context.add_entity::<Person, _>((Age(22),)).unwrap();
        context.set_property::<Person, HomeId>(person1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(person2, HomeId(Some(home_id)));
        context.set_property::<Person, WorkId>(person3, WorkId(Some(work_id)));
        let home_size = context.get_setting_size(home_id).unwrap();
        let work_size = context.get_setting_size(work_id).unwrap();
        assert_eq!(home_size, 2);
        assert_eq!(work_size, 1);
    }

    #[test]
    fn test_get_setting_size_after_removal() {
        let mut context = setup(0.0);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(103), SettingCategory::Home))
            .unwrap();
        let person1 = context.add_entity::<Person, _>((Age(23),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(24),)).unwrap();
        context.set_property::<Person, HomeId>(person1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(person2, HomeId(Some(home_id)));
        let size_before = context.get_setting_size(home_id).unwrap();
        assert_eq!(size_before, 2);
        // Remove person1 from home
        context.set_property::<Person, HomeId>(person1, HomeId(None));
        let size_after = context.get_setting_size(home_id).unwrap();
        assert_eq!(size_after, 1);
    }

    #[test]
    fn test_sample_person_from_home() {
        let mut context = setup(0.0);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(1), SettingCategory::Home))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let sampled_none = context.sample_person_from_setting(home_id);
        assert!(sampled_none.is_err());
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let sampled = context.sample_person_from_setting(home_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_work() {
        let mut context = setup(0.0);
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(2), SettingCategory::Work))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id)));
        let sampled = context.sample_person_from_setting(work_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_school() {
        let mut context = setup(0.0);
        let school_id = context
            .add_entity::<Setting, _>((SettingCode(3), SettingCategory::School))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(10),)).unwrap();
        context.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id)));
        let sampled = context.sample_person_from_setting(school_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_community() {
        let mut context = setup(0.0);
        let community_id = context
            .add_entity::<Setting, _>((SettingCode(4), SettingCategory::Community))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(40),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id)));
        let sampled = context.sample_person_from_setting(community_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_get_setting_size() {
        let mut context = setup(0.0);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(5), SettingCategory::Home))
            .unwrap();
        let person1 = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        context.set_property::<Person, HomeId>(person1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(person2, HomeId(Some(home_id)));
        let size = context.get_setting_size(home_id).unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_get_setting_ratio() {
        let mut context = setup(0.0);
        let school_id = context
            .add_entity::<Setting, _>((SettingCode(7), SettingCategory::School))
            .unwrap();
        let ratio = context.get_setting_ratio(school_id).unwrap();
        assert_eq!(ratio, 0.25);
    }

    // #[test]
    // fn test_get_active_settings_for_person() {
    //     let mut context = setup();
    //     let home_id = context
    //         .add_entity::<Setting, _>((SettingCode(7), Alpha(0.5), SettingCategory::Home))
    //         .unwrap();
    //     let person_id = context.add_entity::<Person, _>((Age(22),)).unwrap();
    //     context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
    //     let active = context.get_active_settings_for_person(person_id).unwrap();
    //     assert!(active.contains(&home_id));
    // }

    #[test]
    fn test_calculate_current_infectiousness_multiplier_for_person() {
        let alpha = 0.5;
        let mut context = setup(alpha);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(8), SettingCategory::Home))
            .unwrap();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(9), SettingCategory::Work))
            .unwrap();
        let p1 = context.add_entity::<Person, _>((Age(23),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(23),)).unwrap();
        let p3 = context.add_entity::<Person, _>((Age(23),)).unwrap();
        context.set_property::<Person, HomeId>(p1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(p2, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(p3, HomeId(Some(home_id)));
        context.set_property::<Person, WorkId>(p1, WorkId(Some(work_id)));
        context.set_property::<Person, WorkId>(p2, WorkId(Some(work_id)));

        let val = context.calculate_current_infectiousness_multiplier_for_person(p1);
        // home size = 3, alpha = 0.5, so multiplier = (3-1)^0.5 = 2^0.5 = 1.41 * 0.5 = 0.707
        // work size = 2, alpha = 0.5, so multiplier = (2-1)^0.5 = 1^0.5 = 1 * 0.5 = 0.5
        assert_almost_eq!(val, 1.207, 0.001);
    }

    #[test]
    fn test_calculate_max_infectiousness_multiplier_for_person() {
        let alpha = 1.0;
        let mut context = setup(alpha);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(9), SettingCategory::Home))
            .unwrap();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(10), SettingCategory::Work))
            .unwrap();
        let p1 = context.add_entity::<Person, _>((Age(24),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(24),)).unwrap();
        context.set_property::<Person, HomeId>(p1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(p2, HomeId(Some(home_id)));
        context.set_property::<Person, WorkId>(p1, WorkId(Some(work_id)));
        let val = context.calculate_max_infectiousness_multiplier_for_person(p1);
        // size = 2, alpha = 1.0, so multiplier = (2-1)^1 = 1
        assert_eq!(val, 1.0);
    }

    #[test]
    fn test_sample_person_from_setting() {
        let mut context = setup(0.0);
        let comm_id = context
            .add_entity::<Setting, _>((SettingCode(10), SettingCategory::Community))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(25),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(comm_id)));
        let sampled = context.sample_person_from_setting(comm_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_from_setting_with_exclusion() {
        let mut context = setup(0.0);
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(11), SettingCategory::Work))
            .unwrap();
        let p1 = context.add_entity::<Person, _>((Age(26),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(27),)).unwrap();
        context.set_property::<Person, WorkId>(p1, WorkId(Some(work_id)));
        context.set_property::<Person, WorkId>(p2, WorkId(Some(work_id)));
        let sampled = context
            .sample_from_setting_with_exclusion(p1, work_id)
            .unwrap();
        assert_eq!(sampled, Some(p2));
    }

    #[test]
    fn test_sample_active_setting() {
        let mut context = setup(0.0);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(13), SettingCategory::Home))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let sampled = context.sample_active_setting(person_id).unwrap();
        assert_eq!(sampled, home_id);
    }

    #[test]
    fn test_add_person_to_setting_and_add_index_setting() {
        let mut context = setup(0.0);
        let person_id = context.add_entity::<Person, _>((Age(31),)).unwrap();
        let setting_code = SettingCode(14);
        context
            .add_person_to_setting(person_id, SettingCategory::Home, setting_code)
            .unwrap();
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let code = context.get_property::<Setting, SettingCode>(home_id);
        assert_eq!(code, setting_code);
    }
}
