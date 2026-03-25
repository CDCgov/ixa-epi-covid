use ixa::{prelude::*};
use serde::{Deserialize, Serialize};

use core::f64;
use std::hash::Hash;

use crate::{Age, ContextParametersExt, Params, parameters::CoreSettingsTypes, population_loader::{CommunityId, HomeId, Person, PersonId, SchoolId, WorkId}};

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
    Community
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub usize);


impl_property!(SettingCode, HomeEntity);
impl_property!(SettingCode, SchoolEntity);
impl_property!(SettingCode, WorkEntity);
impl_property!(SettingCode, CommunityEntity);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum WrappedSettingId {
    Home(HomeEntityId),
    Work(WorkEntityId),
    School(SchoolEntityId),
    Community(CommunityEntityId)
}
// Add settings from the synthetic population file
// Setting properties:
// alpha, setting code, setting category
// Region?
trait ContextSettingExtPrivate: PluginContext + ContextEntitiesExt + ContextParametersExt {
    fn sample_person_from_home(&self, home_id: HomeEntityId) -> Result<PersonId, IxaError> {
        if let Some(sample) = self.sample_entity::<Person, _, _>(SettingRng, (HomeId(Some(home_id)),)) {
            return Ok(sample);
        } else {
            return Err(IxaError::IxaError(format!("No members found for home id: {:?}", home_id)));
        }
    }
    fn sample_person_from_work(&self, work_id: WorkEntityId) -> Result<PersonId, IxaError> {
        if let Some(sample) = self.sample_entity::<Person, _, _>(SettingRng, (WorkId(Some(work_id)),)) {
            return Ok(sample);
        } else {
            return Err(IxaError::IxaError(format!("No members found for work id: {:?}", work_id)));
        }
    }
    fn sample_person_from_school(&self, school_id: SchoolEntityId) -> Result<PersonId, IxaError> {
        if let Some(sample) = self.sample_entity::<Person, _, _>(SettingRng, (SchoolId(Some(school_id)),)) {
            return Ok(sample);
        } else {
            return Err(IxaError::IxaError(format!("No members found for school id: {:?}", school_id)));
        }
    }
    fn sample_person_from_community(&self, community_id: CommunityEntityId) -> Result<PersonId, IxaError> {
        if let Some(sample) = self.sample_entity::<Person, _, _>(SettingRng, (CommunityId(Some(community_id)),)) {
            return Ok(sample);
        } else {
            return Err(IxaError::IxaError(format!("No members found for community id: {:?}", community_id)));
        }
    }
    fn get_setting_alpha(&self, setting: WrappedSettingId) -> Result<f64, IxaError> {
        let Params { settings_properties, .. } = self.get_params();
        match setting {          
            WrappedSettingId::Home(_) => Ok(settings_properties.get(&CoreSettingsTypes::Home).unwrap().alpha),
            WrappedSettingId::School(_) => Ok(settings_properties.get(&CoreSettingsTypes::School).unwrap().alpha),
            WrappedSettingId::Work(_) => Ok(settings_properties.get(&CoreSettingsTypes::Workplace).unwrap().alpha),
            WrappedSettingId::Community(_) => Ok(settings_properties.get(&CoreSettingsTypes::CensusTract).unwrap().alpha),     
        }    
    }

    fn get_setting_ratio(&self, setting: WrappedSettingId) -> Result<f64, IxaError> {
        let Params { itinerary_ratios, .. } = self.get_params();
        match setting {
            WrappedSettingId::Home(_) => Ok(*itinerary_ratios.get(&CoreSettingsTypes::Home).unwrap()),
            WrappedSettingId::School(_) => Ok(*itinerary_ratios.get(&CoreSettingsTypes::School).unwrap()),
            WrappedSettingId::Work(_) => Ok(*itinerary_ratios.get(&CoreSettingsTypes::Workplace).unwrap()),
            WrappedSettingId::Community(_) => Ok(*itinerary_ratios.get(&CoreSettingsTypes::CensusTract).unwrap()),     
        }
    }

}
impl ContextSettingExtPrivate for Context {}

#[allow(private_bounds)]
pub trait ContextSettingExt: PluginContext + ContextEntitiesExt + ContextSettingExtPrivate + ContextParametersExt {
    fn get_setting_size(&self, setting: WrappedSettingId) -> Result<usize, IxaError> {
        match setting {
            WrappedSettingId::Home(home_id) => Ok(self.query_entity_count::<Person,_ >((HomeId(Some(home_id)),))),
            WrappedSettingId::School(school_id) => Ok(self.query_entity_count::<Person,_ >((SchoolId(Some(school_id)),))),
            WrappedSettingId::Work(work_id) => Ok(self.query_entity_count::<Person,_ >((WorkId(Some(work_id)),))),
            WrappedSettingId::Community(community_id) => Ok(self.query_entity_count::<Person,_ >((CommunityId(Some(community_id)),))),     
        }
    }

    fn get_active_settings_for_person(&self, person_id: PersonId) -> Result<Vec<WrappedSettingId>, IxaError> {
        let mut active_settings = Vec::new();
        if let Some(home_id) = self.get_property::<Person, HomeId>(person_id).0 {
            active_settings.push(WrappedSettingId::Home(home_id));
        }
        if let Some(school_id) = self.get_property::<Person, SchoolId>(person_id).0 {
            active_settings.push(WrappedSettingId::School(school_id));
        }
        if let Some(work_id) = self.get_property::<Person, WorkId>(person_id).0 {
            active_settings.push(WrappedSettingId::Work(work_id));
        }
        if let Some(community_id) = self.get_property::<Person, CommunityId>(person_id).0 {
            active_settings.push(WrappedSettingId::Community(community_id));
        }
        Ok(active_settings)
    }

    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut current_inf = 0.0;
        for setting_id in active_settings.iter() {
            let multiplier = self.calculate_multipler(*setting_id).unwrap();
            let ratio = self.get_setting_ratio(*setting_id).unwrap();
            current_inf +=  ratio * multiplier;
        }
        return current_inf;
    }
    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        let active_settings = self.get_active_settings_for_person(person_id).unwrap();
        let mut max_inf = 0.0;
        for setting_id in active_settings.iter() {
            let multiplier = self.calculate_multipler(*setting_id).unwrap();
            max_inf = f64::max(max_inf, multiplier);
        }
        return max_inf;
    }

    fn sample_person_from_setting(
        &self,
        setting: WrappedSettingId,
    ) -> Result<PersonId, IxaError> {
        match setting {
            WrappedSettingId::Home(home_id) => self.sample_person_from_home(home_id),
            WrappedSettingId::School(school_id) => self.sample_person_from_school(school_id),
            WrappedSettingId::Work(work_id) => self.sample_person_from_work(work_id),
            WrappedSettingId::Community(community_id) => self.sample_person_from_community(community_id),     
        }
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: PersonId,
        setting: WrappedSettingId,
    ) -> Result<Option<PersonId>, IxaError> {
        if self.get_setting_size(setting)? == 1 {
            return Ok(None)
        }
        loop {
            let sampled_person = self.sample_person_from_setting(setting)?;
            if sampled_person != person_id {
                return Ok(Some(sampled_person))
            }
        }
    }
    
    fn calculate_multipler(&self, setting: WrappedSettingId) -> Result<f64, IxaError> {
        let alpha = self.get_setting_alpha(setting)?;
        if alpha > 0.0 {
            let size = self.get_setting_size(setting)?;
            return Ok(((size - 1) as f64).powf(alpha));
        }else {
            return Ok(1.0);
        }
    }

    
    fn sample_active_setting(&self, person_id: PersonId) -> Result<WrappedSettingId, IxaError> {
        let mut weights_vec = vec!();
        let ids_vec = self.get_active_settings_for_person(person_id)?;
        let mut sum_weights = 0.0;
        for id in ids_vec.iter() {
            let ratio = self.get_setting_ratio(*id)?;
            let multiplier = self.calculate_multipler(*id)?;
            //println!("Setting {:?} has multiplier: {:?} with ratio: {:?}", id, multiplier, ratio);
            weights_vec.push(multiplier * ratio);
            sum_weights += multiplier * ratio;
        }

        // println!("Person {:?} has weights vec {:?} and ids vec {:?}",
        //     person_id,
        //     weights_vec,
        //     ids_vec
        // );
        if sum_weights > 0.0 {
            let setting_index = self.sample_weighted(SettingRng, &weights_vec);
            Ok(ids_vec[setting_index])
        } else {
            let setting_index = self.sample_range(SettingRng, 0..ids_vec.len());
            Ok(ids_vec[setting_index])
        }
    }
    
    fn add_person_to_setting(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
        setting_code: SettingCode,
    ) -> Result<(), IxaError> {
        let setting_entity_id = self.add_index_setting(setting_category, setting_code)?;
        match setting_entity_id {
            WrappedSettingId::Home(home_id) => self.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id))),
            WrappedSettingId::Work(work_id) => self.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id))),
            WrappedSettingId::School(school_id) => self.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id))),
            WrappedSettingId::Community(community_id) => self.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id))),
        }
        Ok(())
    }
    fn add_index_setting(&mut self, setting_category: SettingCategory, setting_code: SettingCode) -> Result<WrappedSettingId, IxaError> {
        match setting_category {
            SettingCategory::Home => {
                if let Some(setting_id) = self.query_result_iterator::<HomeEntity, _>( (setting_code,)).next() {
                    return Ok(WrappedSettingId::Home(setting_id))
                } else {
                    let setting_id = self.add_entity::<HomeEntity, _>((setting_code,)).unwrap();
                    return Ok(WrappedSettingId::Home(setting_id))
                }
            },
            SettingCategory::School => {
                if let Some(setting_id) = self.query_result_iterator::<SchoolEntity, _>( (setting_code,)).next() {
                    return Ok(WrappedSettingId::School(setting_id))
                } else {
                    let setting_id = self.add_entity::<SchoolEntity, _>((setting_code,)).unwrap();
                    return Ok(WrappedSettingId::School(setting_id))
                }
            },
            SettingCategory::Work => {
                if let Some(setting_id) = self.query_result_iterator::<WorkEntity, _>( (setting_code,)).next() {
                    return Ok(WrappedSettingId::Work(setting_id))
                } else {
                    let setting_id = self.add_entity::<WorkEntity, _>((setting_code,)).unwrap();
                    return Ok(WrappedSettingId::Work(setting_id))
                }
            },
            SettingCategory::Community => {
                if let Some(setting_id) = self.query_result_iterator::<CommunityEntity, _>( (setting_code, )).next() {
                    return Ok(WrappedSettingId::Community(setting_id))
                } else {
                    let setting_id = self.add_entity::<CommunityEntity, _>((setting_code,)).unwrap();
                    return Ok(WrappedSettingId::Community(setting_id))
                }
            },
        }
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.index_property::<HomeEntity, SettingCode>();
    context.index_property::<WorkEntity, SettingCode>();
    context.index_property::<SchoolEntity, SettingCode>();
    context.index_property::<CommunityEntity, SettingCode>();
    
    let p1 = context.add_entity::<Person, _>((Age(0),)).unwrap();
    let p2 = context.add_entity::<Person, _>((Age(0),)).unwrap();
    println!("Person {:?} with home: {:?}, work: {:?}, school: {:?}, comm: {:?}",
        p1,
        context.get_property::<Person, HomeId>(p1),
        context.get_property::<Person, WorkId>(p1),
        context.get_property::<Person, SchoolId>(p1),
        context.get_property::<Person, CommunityId>(p1)        
    );
    // Add work
    let w1: usize = 200;
    // Add school
    let s1: usize = 100;
    // Add Home
    let h1: usize = 315;
    // Add Community
    let c1: usize = 432150001;

    context.add_person_to_setting(p1, SettingCategory::Home, SettingCode(h1))?;
    context.add_person_to_setting(p1, SettingCategory::School, SettingCode(s1))?;
    context.add_person_to_setting(p1, SettingCategory::Work, SettingCode(w1))?;
    context.add_person_to_setting(p1, SettingCategory::Community, SettingCode(c1))?;

    context.add_person_to_setting(p2, SettingCategory::Home, SettingCode(h1))?;
    
    println!("Person {:?} with home: {:?}, work: {:?}, school: {:?}, comm: {:?}",
        p1,
        context.get_property::<HomeEntity, SettingCode>(context.get_property::<Person, HomeId>(p1).0.unwrap()),
        context.get_property::<WorkEntity, SettingCode>(context.get_property::<Person, WorkId>(p1).0.unwrap()),
        context.get_property::<SchoolEntity, SettingCode>(context.get_property::<Person, SchoolId>(p1).0.unwrap()),
        context.get_property::<CommunityEntity, SettingCode>(context.get_property::<Person, CommunityId>(p1).0.unwrap())        
    );    

    let setting_p1 = context.sample_active_setting(p1).unwrap();
    let setting_p2 = context.sample_active_setting(p2).unwrap();
    let settings_p1 = context.get_active_settings_for_person(p1).unwrap();
    let sampled_p = context.sample_from_setting_with_exclusion(p1, setting_p1);

    let sampled_p2 = context.sample_from_setting_with_exclusion(p2, setting_p2);
    println!("Person {:?} active settings: {:?} - sampled: {:?}", p1, settings_p1, setting_p1);
    println!("Setting {:?} - person sampled: {:?}", setting_p1, sampled_p);    
    println!("Person {:?} - setting sampled: {:?}", p2, setting_p2);
    println!("Setting {:?} - person sampled: {:?}", setting_p2, sampled_p2);
    //println!("Person {:?} sampled setting: {:?}", p2, setting_p2);
    // for i in 0..10_000_000 {
    //     let id = (i as f64 / 5.0).floor() as usize;
    //     let p_id = context.add_entity::<Person, _>(()).unwrap();
    //     context.add_person_to_setting(p_id, SettingCategory::Home, SettingCode(id))?;
    //     context.add_person_to_setting(p_id, SettingCategory::School, SettingCode(id))?;
    //     context.add_person_to_setting(p_id, SettingCategory::Work, SettingCode(id))?;
    //     context.add_person_to_setting(p_id, SettingCategory::Community, SettingCode(id))?;   
    // }
    Ok(())
}
// To do: Write tests for each method like the one above in the init
// Write a description with diagram 
// Try to convey the issue that we don't have a generic type of entities (trait/generic/hieracrchy)

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::{CoreSettingsTypes, GlobalParams};
    use ixa::HashMap;

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
            settings_properties: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, SettingProperties { alpha: 0.0 }),
                    (CoreSettingsTypes::School, SettingProperties { alpha: 0.0 }),
                    (
                        CoreSettingsTypes::Workplace,
                        SettingProperties { alpha: 0.0 },
                    ),
                    (
                        CoreSettingsTypes::CensusTract,
                        SettingProperties { alpha: 0.0 },
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (CoreSettingsTypes::Home, 0.25),
                (CoreSettingsTypes::School, 0.25),
                (CoreSettingsTypes::Workplace, 0.25),
                (CoreSettingsTypes::CensusTract, 0.25),
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
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(1),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let sampled_none = context.sample_person_from_home(home_id);
        assert!(sampled_none.is_err());
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let sampled = context.sample_person_from_home(home_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_work() {
        let mut context = setup();
        let work_id = context.add_entity::<WorkEntity, _>((SettingCode(2),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id)));
        let sampled = context.sample_person_from_work(work_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_school() {
        let mut context = setup();
        let school_id = context.add_entity::<SchoolEntity, _>((SettingCode(3),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(10),)).unwrap();
        context.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id)));
        let sampled = context.sample_person_from_school(school_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_community() {
        let mut context = setup();
        let community_id = context.add_entity::<CommunityEntity, _>((SettingCode(4),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(40),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id)));
        let sampled = context.sample_person_from_community(community_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_get_setting_size() {
        let mut context = setup();
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(5),)).unwrap();
        let person1 = context.add_entity::<Person, _>((Age(20),)).unwrap();
        let person2 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        context.set_property::<Person, HomeId>(person1, HomeId(Some(home_id)));
        context.set_property::<Person, HomeId>(person2, HomeId(Some(home_id)));
        let wrapped = WrappedSettingId::Home(home_id);
        let size = context.get_setting_size(wrapped).unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_get_setting_ratio() {
        let mut context = setup();
        let school_id = context.add_entity::<SchoolEntity, _>((SettingCode(7),)).unwrap();
        let wrapped = WrappedSettingId::School(school_id);
        let ratio = context.get_setting_ratio(wrapped).unwrap();
        assert!((ratio - 0.25).abs() < 1e-8);
    }

    #[test]
    fn test_get_active_settings_for_person() {
        let mut context = setup();
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(7),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(22),)).unwrap();
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let active = context.get_active_settings_for_person(person_id).unwrap();
        assert!(active.contains(&WrappedSettingId::Home(home_id)));
    }

    #[test]
    fn test_calculate_current_infectiousness_multiplier_for_person() {
        let mut context = setup();
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(8),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(23),)).unwrap();
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let val = context.calculate_current_infectiousness_multiplier_for_person(person_id);
        // size = 1, alpha = 1.0, ratio = 0.25, so multiplier = (1-1)^1 = 0
        assert_eq!(val, 0.0);
    }

    #[test]
    fn test_calculate_max_infectiousness_multiplier_for_person() {
        let mut context = setup();
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(9),)).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(24),)).unwrap();
        context.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id)));
        let val = context.calculate_max_infectiousness_multiplier_for_person(person_id);
        // size = 1, alpha = 2.0, so multiplier = (1-1)^2 = 0
        assert_eq!(val, 0.0);
    }

    #[test]
    fn test_sample_person_from_setting() {
        let mut context = setup();
        let comm_id = context.add_entity::<CommunityEntity, _>((SettingCode(10), )).unwrap();
        let person_id = context.add_entity::<Person, _>((Age(25),)).unwrap();
        context.set_property::<Person, CommunityId>(person_id, CommunityId(Some(comm_id)));
        let wrapped = WrappedSettingId::Community(comm_id);
        let sampled = context.sample_person_from_setting(wrapped).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_from_setting_with_exclusion() {
        let mut context = setup();
        let work_id = context.add_entity::<WorkEntity, _>((SettingCode(11),)).unwrap();
        let p1 = context.add_entity::<Person, _>((Age(26),)).unwrap();
        let p2 = context.add_entity::<Person, _>((Age(27),)).unwrap();
        context.set_property::<Person, WorkId>(p1, WorkId(Some(work_id)));
        context.set_property::<Person, WorkId>(p2, WorkId(Some(work_id)));
        let wrapped = WrappedSettingId::Work(work_id);
        let sampled = context.sample_from_setting_with_exclusion(p1, wrapped).unwrap();
        assert_eq!(sampled, Some(p2));
    }

    //#[test]
    // fn test_calculate_multipler() {
    //     let mut context = setup();
    //     let home_id = context.add_entity::<HomeEntity, _>((SettingCode(12))).unwrap();
    //     let p1 = context.add_entity::<Person, _>((Age(28),)).unwrap();
    //     let p2 = context.add_entity::<Person, _>((Age(29),)).unwrap();
    //     context.set_property::<Person, HomeId>(p1, HomeId(Some(home_id)));
    //     context.set_property::<Person, HomeId>(p2, HomeId(Some(home_id)));
    //     let wrapped = WrappedSettingId::Home(home_id);
    //     let multiplier = context.calculate_multipler(wrapped).unwrap();
    //     // size = 2, alpha = 2.0, so (2-1)^2 = 1
    //     assert_eq!(multiplier, 1.0);
    // }

    #[test]
    fn test_sample_active_setting() {
        let mut context = setup();
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(13),)).unwrap();
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
        context.add_person_to_setting(person_id, SettingCategory::Home, setting_code).unwrap();
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let code = context.get_property::<HomeEntity, SettingCode>(home_id);
        assert_eq!(code, setting_code);
    }
}
