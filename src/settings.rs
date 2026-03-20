use ixa::{HashSetExt, prelude::*};
use serde::{Deserialize, Serialize};

use core::f64;
use std::hash::Hash;

use crate::{Age, population_loader::{CommunityId, HomeId, Person, PersonId, SchoolId, WorkId}};

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
    Community(CommunityEntityId)
}
// Add settings from the synthetic population file
// Setting properties:
// alpha, setting code, setting category
// Region?
trait ContextSettingExtPrivate: PluginContext + ContextEntitiesExt {
    fn sample_person_from_home(&self, home_id: HomeEntityId) -> Result<PersonId, IxaError> {
        let mut members: Vec<PersonId> = Vec::new();
        self.with_query_results::<Person, _>((HomeId(Some(home_id)),), &mut |results| {
            members = results.to_owned_vec();
        });
        let ind = self.sample_range(SettingRng, 0..members.len());
        Ok(members[ind])
    }
    fn sample_person_from_work(&self, work_id: WorkEntityId) -> Result<PersonId, IxaError> {
        let mut members: Vec<PersonId> = Vec::new();
        self.with_query_results::<Person, _>((WorkId(Some(work_id)),), &mut |results| {
            members = results.to_owned_vec();
        });
        let ind = self.sample_range(SettingRng, 0..members.len());
        Ok(members[ind])
    }
    fn sample_person_from_school(&self, school_id: SchoolEntityId) -> Result<PersonId, IxaError> {
        let mut members: Vec<PersonId> = Vec::new();
        self.with_query_results::<Person, _>((SchoolId(Some(school_id)),), &mut |results| {
            members = results.to_owned_vec();
        });
        let ind = self.sample_range(SettingRng, 0..members.len());
        Ok(members[ind])
    }
    fn sample_person_from_community(&self, community_id: CommunityEntityId) -> Result<PersonId, IxaError> {
        let mut members: Vec<PersonId> = Vec::new();
        self.with_query_results::<Person, _>((CommunityId(Some(community_id)),), &mut |results| {
            members = results.to_owned_vec();
        });
        let ind = self.sample_range(SettingRng, 0..members.len());
        Ok(members[ind])
    }
}
impl ContextSettingExtPrivate for Context {}

#[allow(private_bounds)]
pub trait ContextSettingExt: PluginContext + ContextEntitiesExt + ContextSettingExtPrivate {
    fn calculate_current_infectiousness_multiplier_for_person(&self, _person_id: PersonId) -> f64 {
        return 0.1;
    }
    fn calculate_max_infectiousness_multiplier_for_person(&self, _person_id: PersonId) -> f64 {
        return 0.1;
    }

    // TODO: This needs to sample until not person_id
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

    fn sample_current_setting(&self, person_id: PersonId) -> Result<WrappedSettingId, IxaError> {
        let home_id = self.get_property::<Person, HomeId>(person_id).0.unwrap();
        // let alpha = get_alpha(home_id);
        // let ratio = GlobalParams(ratio::home_id);
        // let members = get_setting_members();
        // let multiplier = ((members - 1) as f64).powf(alpha);
        let _school_id = self.get_property::<Person, SchoolId>(person_id).0.unwrap();
        let _work_id = self.get_property::<Person, WorkId>(person_id).0.unwrap();
        let _community_id = self.get_property::<Person, CommunityId>(person_id).0.unwrap();

        
        Ok(WrappedSettingId::Home(home_id))
    }
    
    fn add_person_to_setting(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
        setting_code: SettingCode,
        alpha: Alpha,
    ) -> Result<(), IxaError> {
        let setting_entity_id = self.add_index_setting(setting_category, setting_code, alpha)?;
        match setting_entity_id {
            WrappedSettingId::Home(home_id) => self.set_property::<Person, HomeId>(person_id, HomeId(Some(home_id))),
            WrappedSettingId::Work(work_id) => self.set_property::<Person, WorkId>(person_id, WorkId(Some(work_id))),
            WrappedSettingId::School(school_id) => self.set_property::<Person, SchoolId>(person_id, SchoolId(Some(school_id))),
            WrappedSettingId::Community(community_id) => self.set_property::<Person, CommunityId>(person_id, CommunityId(Some(community_id))),
        }
        Ok(())
    }
    fn add_index_setting(&mut self, setting_category: SettingCategory, setting_code: SettingCode, alpha: Alpha) -> Result<WrappedSettingId, IxaError> {
        match setting_category {
            SettingCategory::Home => {
                if let Some(setting_id) = self.query_result_iterator::<HomeEntity, _>( (setting_code,)).next() {
                    return Ok(WrappedSettingId::Home(setting_id))
                } else {
                    let setting_id = self.add_entity::<HomeEntity, _>((setting_code, alpha,)).unwrap();
                    return Ok(WrappedSettingId::Home(setting_id))
                }
            },
            SettingCategory::School => {
                if let Some(setting_id) = self.query_result_iterator::<SchoolEntity, _>( (setting_code,)).next() {
                    return Ok(WrappedSettingId::School(setting_id))
                } else {
                    let setting_id = self.add_entity::<SchoolEntity, _>((setting_code, alpha,)).unwrap();
                    return Ok(WrappedSettingId::School(setting_id))
                }
            },
            SettingCategory::Work => {
                if let Some(setting_id) = self.query_result_iterator::<WorkEntity, _>( (setting_code,)).next() {
                    return Ok(WrappedSettingId::Work(setting_id))
                } else {
                    let setting_id = self.add_entity::<WorkEntity, _>((setting_code, alpha,)).unwrap();
                    return Ok(WrappedSettingId::Work(setting_id))
                }
            },
            SettingCategory::Community => {
                if let Some(setting_id) = self.query_result_iterator::<CommunityEntity, _>( (setting_code, )).next() {
                    return Ok(WrappedSettingId::Community(setting_id))
                } else {
                    let setting_id = self.add_entity::<CommunityEntity, _>((setting_code, alpha,)).unwrap();
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

    context.add_person_to_setting(p1, SettingCategory::Home, SettingCode(h1), Alpha(0.1))?;
    context.add_person_to_setting(p1, SettingCategory::School, SettingCode(s1), Alpha(0.1))?;
    context.add_person_to_setting(p1, SettingCategory::Work, SettingCode(w1), Alpha(0.1))?;
    context.add_person_to_setting(p1, SettingCategory::Community, SettingCode(c1), Alpha(0.1))?;
    
    println!("Person {:?} with home: {:?}, work: {:?}, school: {:?}, comm: {:?}",
        p1,
        context.get_property::<HomeEntity, SettingCode>(context.get_property::<Person, HomeId>(p1).0.unwrap()),
        context.get_property::<WorkEntity, SettingCode>(context.get_property::<Person, WorkId>(p1).0.unwrap()),
        context.get_property::<SchoolEntity, SettingCode>(context.get_property::<Person, SchoolId>(p1).0.unwrap()),
        context.get_property::<CommunityEntity, SettingCode>(context.get_property::<Person, CommunityId>(p1).0.unwrap())        
    );    

    // for i in 0..10_000_000 {
    //     let id = (i as f64 / 5.0).floor() as usize;
    //     let p_id = context.add_entity::<Person, _>(()).unwrap();
    //     context.add_person_to_setting(p_id, SettingCategory::Home, SettingCode(id), Alpha(0.1))?;
    //     context.add_person_to_setting(p_id, SettingCategory::School, SettingCode(id), Alpha(0.1))?;
    //     context.add_person_to_setting(p_id, SettingCategory::Work, SettingCode(id), Alpha(0.1))?;
    //     context.add_person_to_setting(p_id, SettingCategory::Community, SettingCode(id), Alpha(0.1))?;   
    // }
    Ok(())
}

