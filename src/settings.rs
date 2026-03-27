use ixa::prelude::*;
use serde::{Deserialize, Serialize};

use core::f64;
use std::{fmt::Debug, hash::Hash};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    population_loader::{CommunityId, HomeId, Itinerary, Person, PersonId, SchoolId, WorkId},
};

define_rng!(SettingRng);

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
}

define_entity!(HomeEntity);
define_entity!(SchoolEntity);
define_entity!(WorkEntity);
define_entity!(CommunityEntity);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq)]
pub enum SettingCategory {
    Home,
    Work,
    School,
    Community,
}
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct Alpha(pub f64);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub usize);

impl_property!(SettingCode, HomeEntity);
impl_property!(SettingCode, SchoolEntity);
impl_property!(SettingCode, WorkEntity);
impl_property!(SettingCode, CommunityEntity);

impl_property!(Alpha, HomeEntity);
impl_property!(Alpha, SchoolEntity);
impl_property!(Alpha, WorkEntity);
impl_property!(Alpha, CommunityEntity);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum WrappedSettingId {
    Home(HomeEntityId),
    Work(WorkEntityId),
    School(SchoolEntityId),
    Community(CommunityEntityId),
}

trait ContextSettingExtPrivate: PluginContext + ContextEntitiesExt + ContextParametersExt {
    fn sample_person_from_setting_internal<T>(&self, wrapped_id: T) -> Result<PersonId, ModelError>
    where
        T: Property<Person> + Itinerary + Debug,
    {
        let wrapped_id_debug = format!("{:?}", wrapped_id);
        if let Some(sample) = self.sample_entity::<Person, _, _>(SettingRng, (wrapped_id,)) {
            Ok(sample)
        } else {
            Err(ModelError::ModelError(format!(
                "No members found for id: {}",
                wrapped_id_debug
            )))
        }
    }
    fn get_setting_alpha(&self, setting: WrappedSettingId) -> Result<f64, ModelError> {
        match setting {
            WrappedSettingId::Home(home_id) => {
                Ok(self.get_property::<HomeEntity, Alpha>(home_id).0)
            }
            WrappedSettingId::School(school_id) => {
                Ok(self.get_property::<SchoolEntity, Alpha>(school_id).0)
            }
            WrappedSettingId::Work(work_id) => {
                Ok(self.get_property::<WorkEntity, Alpha>(work_id).0)
            }
            WrappedSettingId::Community(community_id) => {
                Ok(self.get_property::<CommunityEntity, Alpha>(community_id).0)
            }
        }
    }

    fn get_setting_ratio(&self, setting: WrappedSettingId) -> Result<f64, ModelError> {
        let Params {
            itinerary_ratios, ..
        } = self.get_params();
        match setting {
            WrappedSettingId::Home(_) => Ok(*itinerary_ratios.get(&SettingCategory::Home).unwrap()),
            WrappedSettingId::School(_) => {
                Ok(*itinerary_ratios.get(&SettingCategory::School).unwrap())
            }
            WrappedSettingId::Work(_) => Ok(*itinerary_ratios.get(&SettingCategory::Work).unwrap()),
            WrappedSettingId::Community(_) => {
                Ok(*itinerary_ratios.get(&SettingCategory::Community).unwrap())
            }
        }
    }

    fn get_setting_size_internal(&self, setting: WrappedSettingId) -> Result<usize, ModelError> {
        match setting {
            WrappedSettingId::Home(home_id) => {
                Ok(self.query_entity_count::<Person, _>((HomeId(Some(home_id)),)))
            }
            WrappedSettingId::School(school_id) => {
                Ok(self.query_entity_count::<Person, _>((SchoolId(Some(school_id)),)))
            }
            WrappedSettingId::Work(work_id) => {
                Ok(self.query_entity_count::<Person, _>((WorkId(Some(work_id)),)))
            }
            WrappedSettingId::Community(community_id) => {
                Ok(self.query_entity_count::<Person, _>((CommunityId(Some(community_id)),)))
            }
        }
    }

    fn calculate_multiplier_internal(&self, setting: WrappedSettingId) -> Result<f64, ModelError> {
        let size = self.get_setting_size_internal(setting)?;
        let alpha = self.get_setting_alpha(setting)?;
        Ok(((size - 1) as f64).powf(alpha))
    }

    fn get_itinerary_properties_for_person_by_setting<T>(
        &self,
        person_id: PersonId,
    ) -> Result<Option<(WrappedSettingId, f64, f64)>, ModelError>
    where
        T: Property<Person> + Itinerary + Debug,
    {
        if let Some(setting_id) = self.get_property::<Person, T>(person_id).get_setting_id() {
            let ratio = self.get_setting_ratio(setting_id)?;
            let multiplier = self.calculate_multiplier_internal(setting_id)?;
            Ok(Some((setting_id, ratio, multiplier)))
        } else {
            Ok(None)
        }
    }
}
impl ContextSettingExtPrivate for Context {}

#[allow(private_bounds)]
pub trait ContextSettingExt:
    PluginContext + ContextEntitiesExt + ContextSettingExtPrivate + ContextParametersExt
{
    fn get_setting_size(&self, setting: WrappedSettingId) -> Result<usize, ModelError> {
        self.get_setting_size_internal(setting)
    }

    fn get_active_settings_for_person(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<(WrappedSettingId, f64, f64)>, ModelError> {
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

    fn sample_person_from_setting(
        &self,
        setting: WrappedSettingId,
    ) -> Result<PersonId, ModelError> {
        match setting {
            WrappedSettingId::Home(home_id) => {
                self.sample_person_from_setting_internal(HomeId(Some(home_id)))
            }
            WrappedSettingId::School(school_id) => {
                self.sample_person_from_setting_internal(SchoolId(Some(school_id)))
            }
            WrappedSettingId::Work(work_id) => {
                self.sample_person_from_setting_internal(WorkId(Some(work_id)))
            }
            WrappedSettingId::Community(community_id) => {
                self.sample_person_from_setting_internal(CommunityId(Some(community_id)))
            }
        }
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: PersonId,
        setting: WrappedSettingId,
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

    fn calculate_multiplier(&self, setting: WrappedSettingId) -> Result<f64, ModelError> {
        self.calculate_multiplier_internal(setting)
    }

    fn sample_active_setting(&self, person_id: PersonId) -> Result<WrappedSettingId, ModelError> {
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
        alpha: Alpha,
    ) -> Result<(), ModelError> {
        let setting_entity_id = self.add_index_setting(setting_category, setting_code, alpha)?;
        match setting_entity_id {
            WrappedSettingId::Home(home_id) => {
                self.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)))
            }
            WrappedSettingId::Work(work_id) => {
                self.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id)))
            }
            WrappedSettingId::School(school_id) => {
                self.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id)))
            }
            WrappedSettingId::Community(community_id) => {
                self.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id)))
            }
        }
        Ok(())
    }
    fn add_index_setting(
        &mut self,
        setting_category: SettingCategory,
        setting_code: SettingCode,
        alpha: Alpha,
    ) -> Result<WrappedSettingId, ModelError> {
        match setting_category {
            SettingCategory::Home => {
                if let Some(setting_id) = self
                    .query_result_iterator::<HomeEntity, _>((setting_code,))
                    .next()
                {
                    Ok(WrappedSettingId::Home(setting_id))
                } else {
                    let setting_id = self
                        .add_entity::<HomeEntity, _>((setting_code, alpha))
                        .unwrap();
                    Ok(WrappedSettingId::Home(setting_id))
                }
            }
            SettingCategory::School => {
                if let Some(setting_id) = self
                    .query_result_iterator::<SchoolEntity, _>((setting_code,))
                    .next()
                {
                    Ok(WrappedSettingId::School(setting_id))
                } else {
                    let setting_id = self
                        .add_entity::<SchoolEntity, _>((setting_code, alpha))
                        .unwrap();
                    Ok(WrappedSettingId::School(setting_id))
                }
            }
            SettingCategory::Work => {
                if let Some(setting_id) = self
                    .query_result_iterator::<WorkEntity, _>((setting_code,))
                    .next()
                {
                    Ok(WrappedSettingId::Work(setting_id))
                } else {
                    let setting_id = self
                        .add_entity::<WorkEntity, _>((setting_code, alpha))
                        .unwrap();
                    Ok(WrappedSettingId::Work(setting_id))
                }
            }
            SettingCategory::Community => {
                if let Some(setting_id) = self
                    .query_result_iterator::<CommunityEntity, _>((setting_code,))
                    .next()
                {
                    Ok(WrappedSettingId::Community(setting_id))
                } else {
                    let setting_id = self
                        .add_entity::<CommunityEntity, _>((setting_code, alpha))
                        .unwrap();
                    Ok(WrappedSettingId::Community(setting_id))
                }
            }
        }
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    context.index_property::<HomeEntity, SettingCode>();
    context.index_property::<WorkEntity, SettingCode>();
    context.index_property::<SchoolEntity, SettingCode>();
    context.index_property::<CommunityEntity, SettingCode>();
    Ok(())
}
// To do: Write tests for each method like the one above in the init
// Write a description with diagram
// Try to convey the issue that we don't have a generic type of entities (trait/generic/hieracrchy)

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Age, parameters::GlobalParams};
    use ixa::{HashMap, assert_almost_eq};

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
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
    fn test_sample_person_from_home() {
        let mut context = setup();
        let home_id = context
            .add_entity::<HomeEntity, _>((SettingCode(1), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let sampled_none = context.sample_person_from_setting_internal(HomeId(Some(home_id)));
        assert!(sampled_none.is_err());
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let sampled = context
            .sample_person_from_setting_internal(HomeId(Some(home_id)))
            .unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_work() {
        let mut context = setup();
        let work_id = context
            .add_entity::<WorkEntity, _>((SettingCode(2), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id)));
        let sampled = context
            .sample_person_from_setting_internal(WorkId(Some(work_id)))
            .unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_school() {
        let mut context = setup();
        let school_id = context
            .add_entity::<SchoolEntity, _>((SettingCode(3), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(10),)).unwrap();
        context.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id)));
        let sampled = context
            .sample_person_from_setting_internal(SchoolId(Some(school_id)))
            .unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_community() {
        let mut context = setup();
        let community_id = context
            .add_entity::<CommunityEntity, _>((SettingCode(4), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(40),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id)));
        let sampled = context
            .sample_person_from_setting_internal(CommunityId(Some(community_id)))
            .unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_get_setting_size() {
        let mut context = setup();
        let home_id = context
            .add_entity::<HomeEntity, _>((SettingCode(5), Alpha(0.5)))
            .unwrap();
        let person1 = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        context.set_property::<Person, HomeId>(person1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(person2, HomeId(Some(home_id)));
        let wrapped = WrappedSettingId::Home(home_id);
        let size = context.get_setting_size(wrapped).unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_get_setting_alpha() {
        let mut context = setup();
        let alpha = Alpha(0.7);
        let work_id = context
            .add_entity::<WorkEntity, _>((SettingCode(6), alpha))
            .unwrap();
        let wrapped = WrappedSettingId::Work(work_id);
        let a = context.get_setting_alpha(wrapped).unwrap();
        assert_eq!(a, 0.7);
    }

    #[test]
    fn test_get_setting_ratio() {
        let mut context = setup();
        let school_id = context
            .add_entity::<SchoolEntity, _>((SettingCode(7), Alpha(0.5)))
            .unwrap();
        let wrapped = WrappedSettingId::School(school_id);
        let ratio = context.get_setting_ratio(wrapped).unwrap();
        assert_eq!(ratio, 0.25);
    }

    // #[test]
    // fn test_get_active_settings_for_person() {
    //     let mut context = setup();
    //     let home_id = context
    //         .add_entity::<HomeEntity, _>((SettingCode(7), Alpha(0.5)))
    //         .unwrap();
    //     let person_id = context.add_entity::<Person, _>((Age(22),)).unwrap();
    //     context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
    //     let active = context.get_active_settings_for_person(person_id).unwrap();
    //     assert!(active.contains(&WrappedSettingId::Home(home_id)));
    // }

    #[test]
    fn test_calculate_current_infectiousness_multiplier_for_person() {
        let mut context = setup();
        let home_id = context
            .add_entity::<HomeEntity, _>((SettingCode(8), Alpha(0.5)))
            .unwrap();
        let work_id = context
            .add_entity::<WorkEntity, _>((SettingCode(9), Alpha(0.5)))
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
        let mut context = setup();
        let home_id = context
            .add_entity::<HomeEntity, _>((SettingCode(9), Alpha(1.0)))
            .unwrap();
        let work_id = context
            .add_entity::<WorkEntity, _>((SettingCode(10), Alpha(1.0)))
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
        let mut context = setup();
        let comm_id = context
            .add_entity::<CommunityEntity, _>((SettingCode(10), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(25),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(comm_id)));
        let wrapped = WrappedSettingId::Community(comm_id);
        let sampled = context.sample_person_from_setting(wrapped).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_from_setting_with_exclusion() {
        let mut context = setup();
        let work_id = context
            .add_entity::<WorkEntity, _>((SettingCode(11), Alpha(0.5)))
            .unwrap();
        let p1 = context.add_entity::<Person, _>((Age(26),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(27),)).unwrap();
        context.set_property::<Person, WorkId>(p1, WorkId(Some(work_id)));
        context.set_property::<Person, WorkId>(p2, WorkId(Some(work_id)));
        let wrapped = WrappedSettingId::Work(work_id);
        let sampled = context
            .sample_from_setting_with_exclusion(p1, wrapped)
            .unwrap();
        assert_eq!(sampled, Some(p2));
    }

    #[test]
    fn test_sample_active_setting() {
        let mut context = setup();
        let home_id = context
            .add_entity::<HomeEntity, _>((SettingCode(13), Alpha(0.5)))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let sampled = context.sample_active_setting(person_id).unwrap();
        assert_eq!(sampled, WrappedSettingId::Home(home_id));
    }

    #[test]
    fn test_add_person_to_setting_and_add_index_setting() {
        let mut context = setup();
        let person_id = context.add_entity::<Person, _>((Age(31),)).unwrap();
        let setting_code = SettingCode(14);
        let alpha = Alpha(0.3);
        context
            .add_person_to_setting(person_id, SettingCategory::Home, setting_code, alpha)
            .unwrap();
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let code = context.get_property::<HomeEntity, SettingCode>(home_id);
        assert_eq!(code, setting_code);
    }
}
