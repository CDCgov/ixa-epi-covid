use indexmap::IndexSet;
use ixa::{HashMap, prelude::*};
use serde::{Deserialize, Serialize};

use std::hash::Hash;

define_rng!(SettingRng);
define_entity!(Setting);
define_entity!(Person);


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


#[derive(Default)]
struct SettingsDataContainer {
    settings_list: HashMap<SettingCategory, IndexSet<SettingCode>>,
}

impl SettingsDataContainer {
    fn is_setting_in_registry(&mut self, setting_category: SettingCategory, setting_code: SettingCode) -> bool {
        if let Some(s) = self.settings_list.get(&setting_category) {
            if s.contains(&setting_code) {
                return true
            }
        }
        self.settings_list.entry(setting_category).or_default().insert(setting_code);
        return false;
    }
}
define_data_plugin!(
    SettingsDataPlugin,
    SettingsDataContainer,
    SettingsDataContainer::default()
);

// Add settings from the synthetic population file
// Setting properties:
// alpha, setting code, setting category
// Region?
pub trait SettingsContextExt: PluginContext + ContextEntitiesExt {
    fn add_person_to_setting(
        &mut self,
        person_id: PersonId,
        setting_code: SettingCode
    ) {
        // - Do we need to create the setting? 
        // - record somewhere that person_id is part of setting_id
        // - record that this setting has a membership including the person?
        // -
        // Population loader
        //p1 = context.add_entity::<PersonId, _>((Age(0),)).unwrap();

        // Population loader or settings? 
        //let s1 = context.add_entity::<Setting, _>((SettingCategory::Home, Alpha(0.1),)).unwrap();
        //context.add_person_to_setting(p1, settingId(string));
        println!("{:?} - {:?}", person_id, setting_code);
    }
    fn add_index_setting(&mut self, setting_category: SettingCategory, setting_code: SettingCode, alpha: Alpha) -> Result<SettingId, IxaError> {
        if let Some(setting_id) = self.query_result_iterator::<Setting, _>((setting_category, setting_code)).next() {
            return Ok(setting_id)
        } else {
            let setting_id = self.add_entity::<Setting, _>((setting_category, setting_code, alpha,)).unwrap();
            return Ok(setting_id)
        }
    }
       
 fn add_setting(&mut self, setting_category: SettingCategory, setting_code: SettingCode, alpha: Alpha) -> Result<SettingId, IxaError> {
        // If setting code has already been created, find setting id and return
        // If setting code doesn´t exist, add a new setting and return setting id
        let container = self.get_data_mut(SettingsDataPlugin);
        if !container.is_setting_in_registry(setting_category, setting_code) {
            let setting_id = self.add_entity::<Setting, _>((setting_category, setting_code, alpha,)).unwrap();
            return Ok(setting_id);
        }
        let setting_id = self.query_result_iterator::<Setting, _>((setting_category, setting_code)).next().unwrap();
        Ok(setting_id)
    }
}

impl SettingsContextExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.index_property::<Setting, (SettingCategory, SettingCode)>();
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

    let eh1 = context.add_index_setting(SettingCategory::Home, SettingCode(h1), Alpha(0.1))?;
    context.set_property::<Person, HomeId>(p1, HomeId(Some(eh1)));
    let ew1 = context.add_index_setting(SettingCategory::Work, SettingCode(w1), Alpha(0.2))?;
    context.set_property::<Person, WorkId>(p1, WorkId(Some(ew1)));
    let es1 = context.add_index_setting(SettingCategory::School, SettingCode(s1), Alpha(0.3))?;
    context.set_property::<Person, SchoolId>(p1, SchoolId(Some(es1)));
    let ec1 = context.add_index_setting(SettingCategory::Community, SettingCode(c1), Alpha(0.4))?;
    context.set_property::<Person, CommunityId>(p1, CommunityId(Some(ec1)));


    println!("Person {:?} with home: {:?}, work: {:?}, school: {:?}, comm: {:?}",
        p1,
        context.get_property::<Setting, SettingCode>(context.get_property::<Person, HomeId>(p1).0.unwrap()),
        context.get_property::<Setting, SettingCode>(context.get_property::<Person, WorkId>(p1).0.unwrap()),
        context.get_property::<Setting, SettingCode>(context.get_property::<Person, SchoolId>(p1).0.unwrap()),
        context.get_property::<Setting, SettingCode>(context.get_property::<Person, CommunityId>(p1).0.unwrap())        
    );    

    for _ in 0..1_000_000 {
        let id = context.sample_range(SettingRng, 0..2_000_000) as usize;
        let _ = context.add_index_setting(SettingCategory::Home, SettingCode(id), Alpha(0.1))?;
        let _ = context.add_index_setting(SettingCategory::Work, SettingCode(id), Alpha(0.2))?;
        let _ = context.add_index_setting(SettingCategory::School, SettingCode(id), Alpha(0.3))?;
        let _ = context.add_index_setting(SettingCategory::Community, SettingCode(id), Alpha(0.4))?;
    }
    Ok(())
}

