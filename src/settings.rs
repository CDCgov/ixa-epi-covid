use ixa::{prelude::*};
use serde::{Deserialize, Serialize};

use core::f64;
use std::hash::Hash;

define_rng!(SettingRng);
define_entity!(Setting);
define_entity!(Person);

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


impl_property!(SettingCategory, Setting);
impl_property!(SettingCode, Setting);
impl_property!(Alpha, Setting);

define_multi_property!((SettingCategory, SettingCode), Setting);


#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct AlphaE(pub f64);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCodeE(pub usize);

impl_property!(SettingCodeE, HomeEntity);
impl_property!(SettingCodeE, SchoolEntity);
impl_property!(SettingCodeE, WorkEntity);
impl_property!(SettingCodeE, CommunityEntity);

impl_property!(AlphaE, HomeEntity);
impl_property!(AlphaE, SchoolEntity);
impl_property!(AlphaE, WorkEntity);
impl_property!(AlphaE, CommunityEntity);

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct HomeId(pub Option<SettingId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct WorkId(pub Option<SettingId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct SchoolId(pub Option<SettingId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct CommunityId(pub Option<SettingId>);

impl_property!(HomeId, Person, default_const = HomeId(None));
impl_property!(WorkId, Person, default_const = WorkId(None));
impl_property!(SchoolId, Person, default_const = SchoolId(None));
impl_property!(CommunityId, Person, default_const = CommunityId(None));

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct HomeSingleId(pub Option<HomeEntityId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct WorkSingleId(pub Option<WorkEntityId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct SchoolSingleId(pub Option<SchoolEntityId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct CommunitySingleId(pub Option<CommunityEntityId>);

impl_property!(HomeSingleId, Person, default_const = HomeSingleId(None));
impl_property!(WorkSingleId, Person, default_const = WorkSingleId(None));
impl_property!(SchoolSingleId, Person, default_const = SchoolSingleId(None));
impl_property!(CommunitySingleId, Person, default_const = CommunitySingleId(None));

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
pub trait SettingsContextExt: PluginContext + ContextEntitiesExt {
    fn add_person_to_single_setting(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
        setting_code: SettingCodeE,
        alpha: AlphaE,
    ) -> Result<(), IxaError> {
        let setting_entity_id = self.add_index_single_setting(setting_category, setting_code, alpha)?;
        match setting_entity_id {
            WrappedSettingId::Home(home_id) => self.set_property::<Person, HomeSingleId>(person_id, HomeSingleId(Some(home_id))),
            WrappedSettingId::Work(work_id) => self.set_property::<Person, WorkSingleId>(person_id, WorkSingleId(Some(work_id))),
            WrappedSettingId::School(school_id) => self.set_property::<Person, SchoolSingleId>(person_id, SchoolSingleId(Some(school_id))),
            WrappedSettingId::Community(community_id) => self.set_property::<Person, CommunitySingleId>(person_id, CommunitySingleId(Some(community_id))),
        }
        Ok(())
    }
    fn add_index_single_setting(&mut self, setting_category: SettingCategory, setting_code: SettingCodeE, alpha: AlphaE) -> Result<WrappedSettingId, IxaError> {
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
    fn add_person_to_setting(
        &mut self,
        person_id: PersonId,
        setting_category: SettingCategory,
        setting_code: SettingCode,
        alpha: Alpha,
    ) -> Result<(), IxaError> {
        let setting_entity_id = self.add_index_setting(setting_category, setting_code, alpha)?;
        match setting_category {
            SettingCategory::Home => self.set_property::<Person, HomeId>(person_id, HomeId(Some(setting_entity_id))),
            SettingCategory::Work => self.set_property::<Person, WorkId>(person_id, WorkId(Some(setting_entity_id))),
            SettingCategory::School => self.set_property::<Person, SchoolId>(person_id, SchoolId(Some(setting_entity_id))),
            SettingCategory::Community => self.set_property::<Person, CommunityId>(person_id, CommunityId(Some(setting_entity_id))),
        }
        Ok(())
    }
    fn add_index_setting(&mut self, setting_category: SettingCategory, setting_code: SettingCode, alpha: Alpha) -> Result<SettingId, IxaError> {
        if let Some(setting_id) = self.query_result_iterator::<Setting, _>((setting_category, setting_code)).next() {
            return Ok(setting_id)
        } else {
            let setting_id = self.add_entity::<Setting, _>((setting_category, setting_code, alpha,)).unwrap();
            return Ok(setting_id)
        }
    }       
}

impl SettingsContextExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.index_property::<Setting, (SettingCategory, SettingCode)>();

    context.index_property::<HomeEntity, SettingCodeE>();
    context.index_property::<WorkEntity, SettingCodeE>();
    context.index_property::<SchoolEntity, SettingCodeE>();
    context.index_property::<CommunityEntity, SettingCodeE>();
    
    let p1 = context.add_entity::<Person, _>(()).unwrap();
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

    context.add_person_to_single_setting(p1, SettingCategory::Home, SettingCodeE(h1), AlphaE(0.1))?;
    context.add_person_to_single_setting(p1, SettingCategory::School, SettingCodeE(s1), AlphaE(0.1))?;
    context.add_person_to_single_setting(p1, SettingCategory::Work, SettingCodeE(w1), AlphaE(0.1))?;
    context.add_person_to_single_setting(p1, SettingCategory::Community, SettingCodeE(c1), AlphaE(0.1))?;
    
    println!("Person {:?} with home: {:?}, work: {:?}, school: {:?}, comm: {:?}",
        p1,
        context.get_property::<HomeEntity, SettingCodeE>(context.get_property::<Person, HomeSingleId>(p1).0.unwrap()),
        context.get_property::<WorkEntity, SettingCodeE>(context.get_property::<Person, WorkSingleId>(p1).0.unwrap()),
        context.get_property::<SchoolEntity, SettingCodeE>(context.get_property::<Person, SchoolSingleId>(p1).0.unwrap()),
        context.get_property::<CommunityEntity, SettingCodeE>(context.get_property::<Person, CommunitySingleId>(p1).0.unwrap())        
    );    

    for i in 0..1_000_000 {
        let id = (i as f64 / 5.0).floor() as usize;
        let p_id = context.add_entity::<Person, _>(()).unwrap();
        //let id = context.sample_range(SettingRng, 0..2_000) as usize;
        context.add_person_to_setting(p_id, SettingCategory::Home, SettingCode(id), Alpha(0.1))?;
        context.add_person_to_setting(p_id, SettingCategory::School, SettingCode(id), Alpha(0.1))?;
        context.add_person_to_setting(p_id, SettingCategory::Work, SettingCode(id), Alpha(0.1))?;
        context.add_person_to_setting(p_id, SettingCategory::Community, SettingCode(id), Alpha(0.1))?;

        // context.add_person_to_single_setting(p_id, SettingCategory::Home, SettingCodeE(id), AlphaE(0.1))?;
        // context.add_person_to_single_setting(p_id, SettingCategory::School, SettingCodeE(id), AlphaE(0.1))?;
        // context.add_person_to_single_setting(p_id, SettingCategory::Work, SettingCodeE(id), AlphaE(0.1))?;
        // context.add_person_to_single_setting(p_id, SettingCategory::Community, SettingCodeE(id), AlphaE(0.1))?;   
    }
    Ok(())
}

