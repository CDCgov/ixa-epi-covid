use ixa::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount as EnumCountMacro, EnumIter};

use core::f64;

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    population_loader::{
        CommunityId, HomeId, ItineraryRatios, Person, PersonId, SchoolId, SettingIds, WorkId,
    },
};

define_rng!(SettingRng);

define_entity!(Setting);

#[derive(
    Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq, EnumCountMacro, EnumIter,
)]
#[repr(u8)]
pub enum SettingCategory {
    Home = 0,
    Work,
    School,
    Community,
}

// Implement immutable indexing
impl<T> Index<SettingCategory> for [T; SETTING_COUNT] {
    type Output = T;
    fn index(&self, index: SettingCategory) -> &Self::Output {
        &self[index as usize]
    }
}

// Implement mutable indexing
impl<T> IndexMut<SettingCategory> for [T; SETTING_COUNT] {
    fn index_mut(&mut self, index: SettingCategory) -> &mut Self::Output {
        &mut self[index as usize]
    }
}

pub const SETTING_COUNT: usize = SettingCategory::COUNT;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub usize);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct Size(pub usize);

impl_property!(Size, Setting, default_const = Size(0));

impl_property!(SettingCode, Setting);

impl_property!(SettingCategory, Setting);

define_multi_property!((SettingCategory, SettingCode), Setting);

define_global_property!(SettingAlphas, [f64; SETTING_COUNT]);
define_global_property!(SettingRatios, [f64; SETTING_COUNT]);

trait ContextSettingExtPrivate: PluginContext + ContextEntitiesExt + ContextParametersExt {
    fn get_setting_ratio(&self, setting_category: SettingCategory) -> Result<f64, ModelError> {
        let ratios = self.get_global_property_value(SettingRatios).unwrap();
        Ok(ratios[setting_category])
    }

    fn get_setting_alpha(&self, setting_category: SettingCategory) -> Result<f64, ModelError> {
        let alphas = self.get_global_property_value(SettingAlphas).unwrap();
        Ok(alphas[setting_category])
    }

    fn sample_person_from_setting_internal<T>(&self, setting: T) -> Result<PersonId, ModelError>
    where
        T: Property<Person> + Debug,
    {
        self.sample_entity::<Person, _, _>(SettingRng, (setting,))
            .ok_or_else(|| {
                ModelError::ModelError(format!("No members found for setting: {:?}", setting))
            })
    }

    fn set_setting_size_internal<T>(&mut self, setting: T) -> Result<usize, ModelError>
    where
        T: Property<Person> + Debug,
    {
        Ok(self.query_entity_count::<Person, _>((setting,)))
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
                    .get(&SettingCategory::Work)
                    .unwrap()
                    .alpha,
                settings_properties
                    .get(&SettingCategory::School)
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
                *itinerary_ratios.get(&SettingCategory::Work).unwrap(),
                *itinerary_ratios.get(&SettingCategory::School).unwrap(),
                *itinerary_ratios.get(&SettingCategory::Community).unwrap(),
            ],
        )
        .unwrap();
    }

    fn initialize_setting_size(&mut self) -> Result<(), ModelError> {
        for setting in self.get_entity_iterator::<Setting>() {
            let setting_category = self.get_property::<Setting, SettingCategory>(setting);
            match setting_category {
                SettingCategory::Home => {
                    let size = self.set_setting_size_internal(HomeId(Some(setting)))?;
                    self.set_property::<Setting, Size>(setting, Size(size));
                }
                SettingCategory::School => {
                    let size = self.set_setting_size_internal(SchoolId(Some(setting)))?;
                    self.set_property::<Setting, Size>(setting, Size(size));
                }
                SettingCategory::Work => {
                    let size = self.set_setting_size_internal(WorkId(Some(setting)))?;
                    self.set_property::<Setting, Size>(setting, Size(size));
                }
                SettingCategory::Community => {
                    let size = self.set_setting_size_internal(CommunityId(Some(setting)))?;
                    self.set_property::<Setting, Size>(setting, Size(size));
                }
            }
        }
        Ok(())
    }

    fn get_setting_size(&self, setting: SettingId) -> Result<usize, ModelError> {
        Ok(self.get_property::<Setting, Size>(setting).0)
    }

    fn get_active_settings_for_person(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<(SettingId, f64, f64)>, ModelError> {
        let mut active_settings = Vec::new();
        let setting_ids = self.get_property::<Person, SettingIds>(person_id);
        let itinerary_ratios = self.get_property::<Person, ItineraryRatios>(person_id);
        for category in SettingCategory::iter() {
            if let Some(id) = setting_ids.setting_ids[category] {
                let ratio = itinerary_ratios.itinerary_ratios[category];
                let multiplier = self.calculate_multiplier(id, category)?;
                active_settings.push((id, ratio, multiplier));
            }
        }
        Ok(active_settings)
    }

    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        // active settings is a vector of (setting_id, ratio, multiplier) for the person.
        // When iterating through this vector s.0 refers to the setting_id, s.1 refers to ratio of time the person spends in the setting
        // and s.2 refers to the multiplier for that setting based on its size and alpha.
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut current_inf = 0.0;
        let mut sum_ratio = 0.0;
        // we calculate sum of ratios to normalize weights
        for setting in active_settings.iter() {
            let ratio = setting.1;
            sum_ratio += ratio;
        }
        // Current infectiousness multiplier is the weighted average of the setting specific multipliers
        // where weights are given by the ratio of time spent in each setting normalized by the sum of ratios
        // across active settings.
        if sum_ratio > 0.0 {
            for setting in active_settings.iter() {
                let ratio = setting.1;
                let multiplier = setting.2;
                current_inf += multiplier * (ratio / sum_ratio);
            }
        }
        current_inf
    }

    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        // active settings is a vector of (setting_id, ratio, multiplier) for the person.
        // When iterating through this vector s.0 refers to the setting_id, s.1 refers to ratio of time the person spends in the setting
        // and s.2 refers to the multiplier for that setting based on its size and alpha.
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut max_inf = 0.0;
        for setting in active_settings.iter() {
            let multiplier = setting.2;
            max_inf = f64::max(max_inf, multiplier);
        }
        max_inf
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

    fn calculate_multiplier(
        &self,
        setting: SettingId,
        setting_category: SettingCategory,
    ) -> Result<f64, ModelError> {
        let size = self.get_setting_size(setting)? as f64 - 1.0;
        let alpha = self.get_setting_alpha(setting_category)?;
        Ok(size.powf(alpha))
    }

    fn sample_active_setting(&self, person_id: PersonId) -> Result<SettingId, ModelError> {
        let active_settings = self.get_active_settings_for_person(person_id)?;
        let mut weights_vec = vec![];
        for setting in active_settings.iter() {
            let ratio = setting.1;
            let multiplier = setting.2;
            weights_vec.push(ratio * multiplier);
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
        setting_ids: &mut [Option<SettingId>; SETTING_COUNT],
        itinerary_ratios: &mut [f64; SETTING_COUNT],
        setting_id_parsed: Option<usize>,
        setting_category: SettingCategory,
    ) {
        if setting_id_parsed.is_none() {
            return;
        }
        let setting_id = self
            .add_index_setting(setting_category, SettingCode(setting_id_parsed.unwrap()))
            .unwrap();
        setting_ids[setting_category] = Some(setting_id);
        itinerary_ratios[setting_category] = self.get_setting_ratio(setting_category).unwrap();
    }

    fn add_person_to_settings(
        &mut self,
        person_id: PersonId,
        home_id: Option<usize>,
        work_id: Option<usize>,
        school_id: Option<usize>,
        community_id: Option<usize>,
    ) -> Result<(), ModelError> {
        let mut setting_ids = [None; SETTING_COUNT];
        let mut itinerary_ratios = [0.0; SETTING_COUNT];
        self.add_person_to_setting(
            &mut setting_ids,
            &mut itinerary_ratios,
            home_id,
            SettingCategory::Home,
        );
        self.add_person_to_setting(
            &mut setting_ids,
            &mut itinerary_ratios,
            work_id,
            SettingCategory::Work,
        );
        self.add_person_to_setting(
            &mut setting_ids,
            &mut itinerary_ratios,
            school_id,
            SettingCategory::School,
        );
        self.add_person_to_setting(
            &mut setting_ids,
            &mut itinerary_ratios,
            community_id,
            SettingCategory::Community,
        );
        let sum_ratio = itinerary_ratios.iter().sum::<f64>();
        let normalized_itinerary_ratios = itinerary_ratios.map(|ratio| ratio / sum_ratio);
        self.set_property::<Person, SettingIds>(person_id, SettingIds { setting_ids });
        self.set_property::<Person, ItineraryRatios>(
            person_id,
            ItineraryRatios {
                itinerary_ratios: normalized_itinerary_ratios,
            },
        );
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
                .add_entity::<Setting, _>((setting_category, setting_code))
                .unwrap();
            Ok(setting_id)
        }
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.index_property::<Setting, (SettingCategory, SettingCode)>();
    context.register_setting_global_properties();
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        Age, Params,
        parameters::{GlobalParams, SettingProperties},
    };
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

    fn assign_person_settings(
        context: &mut Context,
        person_id: PersonId,
        assignments: &[(SettingCategory, SettingId)],
        itinerary_ratios: [f64; SETTING_COUNT],
    ) {
        let mut setting_ids = [None; SETTING_COUNT];
        for (category, setting_id) in assignments {
            setting_ids[*category] = Some(*setting_id);
        }
        context.set_property::<Person, SettingIds>(person_id, SettingIds { setting_ids });
        context.set_property::<Person, ItineraryRatios>(
            person_id,
            ItineraryRatios { itinerary_ratios },
        );
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
        let person1 = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        let person3 = context.add_entity::<Person, _>((Age(22),)).unwrap();
        context
            .add_person_to_settings(person1, Some(1), None, None, Some(1))
            .unwrap();
        context
            .add_person_to_settings(person2, Some(1), None, None, Some(1))
            .unwrap();
        context
            .add_person_to_settings(person3, None, Some(1), None, None)
            .unwrap();
        let home_id = context.get_property::<Person, HomeId>(person1).0.unwrap();
        let work_id = context.get_property::<Person, WorkId>(person3).0.unwrap();
        context.initialize_setting_size().unwrap();
        let home_size = context.get_setting_size(home_id).unwrap();
        let work_size = context.get_setting_size(work_id).unwrap();
        assert_eq!(home_size, 2);
        assert_eq!(work_size, 1);
    }

    #[test]
    fn test_get_setting_size_after_removal() {
        let mut context = setup(0.0);
        let person1 = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        context
            .add_person_to_settings(person1, Some(1), None, None, Some(1))
            .unwrap();
        context
            .add_person_to_settings(person2, Some(1), None, None, Some(1))
            .unwrap();
        let home_id = context.get_property::<Person, HomeId>(person1).0.unwrap();
        let community_id = context
            .get_property::<Person, CommunityId>(person1)
            .0
            .unwrap();
        context.initialize_setting_size().unwrap();
        let home_size = context.get_setting_size(home_id).unwrap();
        let community_size = context.get_setting_size(community_id).unwrap();
        assert_eq!(home_size, 2);
        assert_eq!(community_size, 2);
        context
            .add_person_to_settings(person1, Some(2), None, None, None)
            .unwrap();
        context.initialize_setting_size().unwrap();
        let home_size_after = context.get_setting_size(home_id).unwrap();
        let community_size_after = context.get_setting_size(community_id).unwrap();
        assert_eq!(home_size_after, 1);
        assert_eq!(community_size_after, 1);
    }

    #[test]
    fn test_sample_person_from_home() {
        let mut context = setup(0.0);
        let person_id = context.add_entity::<Person, _>((Age(20),)).unwrap();
        context
            .add_person_to_settings(person_id, Some(123), None, None, None)
            .unwrap();
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let sampled = context.sample_person_from_setting(home_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_work() {
        let mut context = setup(0.0);
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context
            .add_person_to_settings(person_id, None, Some(123), None, None)
            .unwrap();
        let work_id = context.get_property::<Person, WorkId>(person_id).0.unwrap();
        let sampled = context.sample_person_from_setting(work_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_school() {
        let mut context = setup(0.0);
        let person_id = context.add_entity::<Person, _>((Age(10),)).unwrap();
        context
            .add_person_to_settings(person_id, None, None, Some(123), None)
            .unwrap();
        let school_id = context
            .get_property::<Person, SchoolId>(person_id)
            .0
            .unwrap();
        let sampled = context.sample_person_from_setting(school_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_community() {
        let mut context = setup(0.0);
        let person_id = context.add_entity::<Person, _>((Age(40),)).unwrap();
        context
            .add_person_to_settings(person_id, None, None, None, Some(123))
            .unwrap();
        let community_id = context
            .get_property::<Person, CommunityId>(person_id)
            .0
            .unwrap();
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
        assign_person_settings(
            &mut context,
            person1,
            &[(SettingCategory::Home, home_id)],
            [1.0, 0.0, 0.0, 0.0],
        );
        assign_person_settings(
            &mut context,
            person2,
            &[(SettingCategory::Home, home_id)],
            [1.0, 0.0, 0.0, 0.0],
        );
        context.initialize_setting_size().unwrap();
        let size = context.get_setting_size(home_id).unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_get_setting_ratio() {
        let context = setup(0.0);
        let ratio = context.get_setting_ratio(SettingCategory::School).unwrap();
        assert_eq!(ratio, 0.25);
    }

    #[test]
    fn test_get_active_settings_for_person() {
        let mut context = setup(0.5);
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(7), SettingCategory::Home))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(22),)).unwrap();
        assign_person_settings(
            &mut context,
            person_id,
            &[(SettingCategory::Home, home_id)],
            [0.25, 0.0, 0.0, 0.0],
        );
        context.initialize_setting_size().unwrap();
        let active = context.get_active_settings_for_person(person_id).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, home_id);
        assert_eq!(active[0].1, 0.25); // default ratio for home is 0.25
        assert_eq!(active[0].2, 0.0); // alpha is set to 0.5 size is set to 1 so the multipler is (1-1)^0.5 = 0
    }

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
        assign_person_settings(
            &mut context,
            p1,
            &[
                (SettingCategory::Home, home_id),
                (SettingCategory::Work, work_id),
            ],
            [0.5, 0.5, 0.0, 0.0],
        );
        assign_person_settings(
            &mut context,
            p2,
            &[
                (SettingCategory::Home, home_id),
                (SettingCategory::Work, work_id),
            ],
            [0.5, 0.5, 0.0, 0.0],
        );
        assign_person_settings(
            &mut context,
            p3,
            &[(SettingCategory::Home, home_id)],
            [1.0, 0.0, 0.0, 0.0],
        );
        context.initialize_setting_size().unwrap();

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
        assign_person_settings(
            &mut context,
            p1,
            &[
                (SettingCategory::Home, home_id),
                (SettingCategory::Work, work_id),
            ],
            [0.5, 0.5, 0.0, 0.0],
        );
        assign_person_settings(
            &mut context,
            p2,
            &[(SettingCategory::Home, home_id)],
            [1.0, 0.0, 0.0, 0.0],
        );
        context.initialize_setting_size().unwrap();
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
        assign_person_settings(
            &mut context,
            person_id,
            &[(SettingCategory::Community, comm_id)],
            [0.0, 0.0, 0.0, 1.0],
        );
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
        assign_person_settings(
            &mut context,
            p1,
            &[(SettingCategory::Work, work_id)],
            [0.0, 1.0, 0.0, 0.0],
        );
        assign_person_settings(
            &mut context,
            p2,
            &[(SettingCategory::Work, work_id)],
            [0.0, 1.0, 0.0, 0.0],
        );
        context.initialize_setting_size().unwrap();
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
        assign_person_settings(
            &mut context,
            person_id,
            &[(SettingCategory::Home, home_id)],
            [1.0, 0.0, 0.0, 0.0],
        );
        context.initialize_setting_size().unwrap();
        let sampled = context.sample_active_setting(person_id).unwrap();
        assert_eq!(sampled, home_id);
    }

    #[test]
    fn test_add_person_to_setting_and_add_index_setting() {
        let mut context = setup(0.0);
        let person_id = context.add_entity::<Person, _>((Age(31),)).unwrap();
        context
            .add_person_to_settings(person_id, Some(123), None, None, None)
            .unwrap();
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let setting_ids = context
            .get_property::<Person, SettingIds>(person_id)
            .setting_ids;
        let itinerary_ratios = context
            .get_property::<Person, ItineraryRatios>(person_id)
            .itinerary_ratios;
        assert_eq!(setting_ids[SettingCategory::Home], Some(home_id));
        println!("itinerary_ratios: {:?}", itinerary_ratios);
        assert_eq!(itinerary_ratios[SettingCategory::Home], 1.0);
        assert_eq!(setting_ids[SettingCategory::Work], None);
        assert_eq!(setting_ids[SettingCategory::School], None);
        assert_eq!(setting_ids[SettingCategory::Community], None);
        assert_eq!(itinerary_ratios[SettingCategory::Work], 0.0);
        assert_eq!(itinerary_ratios[SettingCategory::School], 0.0);
        assert_eq!(itinerary_ratios[SettingCategory::Community], 0.0);
    }
}
