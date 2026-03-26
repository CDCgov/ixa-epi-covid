use ixa::{impl_derived_property, prelude::*};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug};

use core::f64;
use std::hash::Hash;

use crate::{
    ContextParametersExt,
    parameters::GlobalParams,
    population_loader::{CommunityId, HomeId, Itinerary, Person, PersonId, SchoolId, WorkId},
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
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct Alpha(pub f64);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub usize);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct Size(pub usize);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct Multiplier(pub f64);

impl_property!(Size, Setting, default_const = Size(0));

impl_property!(SettingCode, Setting);

impl_property!(Alpha, Setting);

impl_property!(SettingCategory, Setting);

impl_derived_property!(
    Multiplier,
    Setting,
    [Size, SettingCategory],
    [GlobalParams],
    |size, setting_category, params| {
        if size.0 > 0 {
            let alpha = params
                .settings_properties
                .get(&setting_category)
                .unwrap()
                .alpha;
            return Multiplier(((size.0 - 1) as f64).powf(alpha));
        }
        Multiplier(0.0)
    }
);

#[derive(Default)]
struct SettingDataContainer {
    setting_ratios: HashMap<SettingCategory, f64>,
}

impl SettingDataContainer {}

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer::default()
);

// Add settings from the synthetic population file
// Setting properties:
// alpha, setting code, setting category
// Region?
trait ContextSettingExtPrivate: PluginContext + ContextEntitiesExt + ContextParametersExt {
    fn get_setting_ratio(&self, setting: SettingId) -> Result<f64, IxaError> {
        let setting_category = self.get_property::<Setting, SettingCategory>(setting);
        let containter = self.get_data(SettingDataPlugin);
        Ok(*containter.setting_ratios.get(&setting_category).unwrap())
    }

    fn sample_person_from_setting_internal<T>(&self, setting: T) -> Result<PersonId, IxaError>
    where
        T: Property<Person> + Itinerary + Debug,
    {
        self.sample_entity::<Person, _, _>(SettingRng, (setting,))
            .ok_or_else(|| {
                IxaError::IxaError(format!("No members found for setting: {:?}", setting))
            })
    }

    fn get_itinerary_properties_for_person_by_setting<T>(
        &self,
        person_id: PersonId,
    ) -> Result<Option<(SettingId, f64, f64)>, IxaError>
    where
        T: Property<Person> + Itinerary + Debug,
    {
        if let Some(setting_id) = self.get_property::<Person, T>(person_id).get_setting_id() {
            let ratio = self.get_setting_ratio(setting_id)?;
            let multiplier = self.get_property::<Setting, Multiplier>(setting_id).0;
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
    fn register_setting_ratios(&mut self) -> Result<(), IxaError> {
        let params = self.get_params().clone();
        let container = self.get_data_mut(SettingDataPlugin);
        for (setting_category, ratio) in params.itinerary_ratios.iter() {
            container.setting_ratios.insert(*setting_category, *ratio);
        }
        Ok(())
    }
    fn get_setting_size(&self, setting: SettingId) -> Result<usize, IxaError> {
        Ok(self.get_property::<Setting, Size>(setting).0)
    }

    fn get_active_settings_for_person(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<(SettingId, f64, f64)>, IxaError> {
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

    fn sample_person_from_setting(&self, setting: SettingId) -> Result<PersonId, IxaError> {
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
        Ok(self.get_property::<Setting, Multiplier>(setting).0)
    }

    fn sample_active_setting(&self, person_id: PersonId) -> Result<SettingId, IxaError> {
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
    ) -> Result<(), IxaError> {
        let setting_entity_id = self.add_index_setting(setting_category, setting_code, alpha)?;
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
        alpha: Alpha,
    ) -> Result<SettingId, IxaError> {
        if let Some(setting_id) = self
            .query_result_iterator::<Setting, _>((setting_code, setting_category))
            .next()
        {
            Ok(setting_id)
        } else {
            let setting_id = self
                .add_entity::<Setting, _>((setting_code, alpha, setting_category))
                .unwrap();
            Ok(setting_id)
        }
    }

    fn subscribe_to_setting_change<T>(&mut self)
    where
        T: Property<Person> + Itinerary + Debug,
    {
        self.subscribe_to_event(move |context, event: PropertyChangeEvent<Person, T>| {
            if let Some(setting_id) = event.current.get_setting_id() {
                let size = context.get_property::<Setting, Size>(setting_id).0;
                context.set_property::<Setting, Size>(setting_id, Size(size + 1));
            } else if let Some(previous_setting_id) = event.previous.get_setting_id() {
                let size = context.get_property::<Setting, Size>(previous_setting_id).0;
                context.set_property::<Setting, Size>(previous_setting_id, Size(size - 1));
            }
        });
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.index_property::<Setting, SettingCode>();
    context.index_property::<Setting, SettingCategory>();

    context.subscribe_to_setting_change::<HomeId>();
    context.subscribe_to_setting_change::<WorkId>();
    context.subscribe_to_setting_change::<SchoolId>();
    context.subscribe_to_setting_change::<CommunityId>();
    context.register_setting_ratios()?;
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
    fn test_get_setting_size_empty() {
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(100), Alpha(0.5), SettingCategory::Home))
            .unwrap();
        // No persons assigned yet
        let size = context.get_setting_size(home_id).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_get_setting_size_multiple_categories() {
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(101), Alpha(0.5), SettingCategory::Home))
            .unwrap();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(102), Alpha(0.5), SettingCategory::Work))
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
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(103), Alpha(0.5), SettingCategory::Home))
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
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(1), Alpha(0.5), SettingCategory::Home))
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
        let mut context = setup();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(2), Alpha(0.5), SettingCategory::Work))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id)));
        let sampled = context.sample_person_from_setting(work_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_school() {
        let mut context = setup();
        let school_id = context
            .add_entity::<Setting, _>((SettingCode(3), Alpha(0.5), SettingCategory::School))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(10),)).unwrap();
        context.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id)));
        let sampled = context.sample_person_from_setting(school_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_community() {
        let mut context = setup();
        let community_id = context
            .add_entity::<Setting, _>((SettingCode(4), Alpha(0.5), SettingCategory::Community))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(40),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id)));
        let sampled = context.sample_person_from_setting(community_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_get_setting_size() {
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(5), Alpha(0.5), SettingCategory::Home))
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
        let mut context = setup();
        let school_id = context
            .add_entity::<Setting, _>((SettingCode(7), Alpha(0.5), SettingCategory::School))
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
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(8), Alpha(0.5), SettingCategory::Home))
            .unwrap();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(9), Alpha(0.5), SettingCategory::Work))
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
            .add_entity::<Setting, _>((SettingCode(9), Alpha(1.0), SettingCategory::Home))
            .unwrap();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(10), Alpha(1.0), SettingCategory::Work))
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
            .add_entity::<Setting, _>((SettingCode(10), Alpha(0.5), SettingCategory::Community))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(25),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(comm_id)));
        let sampled = context.sample_person_from_setting(comm_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_from_setting_with_exclusion() {
        let mut context = setup();
        let work_id = context
            .add_entity::<Setting, _>((SettingCode(11), Alpha(0.5), SettingCategory::Work))
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
        let mut context = setup();
        let home_id = context
            .add_entity::<Setting, _>((SettingCode(13), Alpha(0.5), SettingCategory::Home))
            .unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let sampled = context.sample_active_setting(person_id).unwrap();
        assert_eq!(sampled, home_id);
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
        let code = context.get_property::<Setting, SettingCode>(home_id);
        assert_eq!(code, setting_code);
    }
}
