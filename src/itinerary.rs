use ixa::{entity::EntitySetIterator, impl_derived_property, prelude::*};

use serde::{Deserialize, Serialize};


use crate::{
    population_loader::{PersonId}, settings_entities::{SettingCategory, HomeId, SchoolId, WorkplaceId, CensusTractId},
};

define_entity!(Itinerary);
define_rng!(ItineraryRng);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub struct BelongsTo(pub PersonId);
impl_property!(BelongsTo, Itinerary);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
pub struct Activity(pub bool);
impl_property!(Activity, Itinerary, default_const = Activity(true));

define_multi_property!((BelongsTo, Activity), Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct HomeItinerary {
    pub home_id: HomeId,
    pub ratio: f64,
}
impl_property!(HomeItinerary, Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct WorkplaceItinerary {
    pub workplace_id: Option<WorkplaceId>,
    pub ratio: Option<f64>,
}
impl_property!(WorkplaceItinerary, Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct SchoolItinerary {
    pub school_id: Option<SchoolId>,
    pub ratio: Option<f64>,
}
impl_property!(SchoolItinerary, Itinerary);
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct CensusTractItinerary {
    pub census_tract_id: CensusTractId,
    pub ratio: f64,
}
impl_property!(CensusTractItinerary, Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct ItineraryHomeId(pub HomeId);
impl_derived_property!(
    ItineraryHomeId,
    Itinerary,
    [HomeItinerary],
    [],
    |home_itinerary| ItineraryHomeId(home_itinerary.home_id)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct ItinerarySchoolId(pub Option<SchoolId>);
impl_derived_property!(
    ItinerarySchoolId,
    Itinerary,
    [SchoolItinerary],
    [],
    |school_itinerary| ItinerarySchoolId(school_itinerary.school_id)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct ItineraryWorkplaceId(pub Option<WorkplaceId>);
impl_derived_property!(
    ItineraryWorkplaceId,
    Itinerary,
    [WorkplaceItinerary],
    [],
    |workplace_itinerary| ItineraryWorkplaceId(workplace_itinerary.workplace_id)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct ItineraryCensusTractId(pub CensusTractId);
impl_derived_property!(
    ItineraryCensusTractId,
    Itinerary,
    [CensusTractItinerary],
    [],
    |census_tract_itinerary| ItineraryCensusTractId(census_tract_itinerary.census_tract_id)
);


pub trait ContextItineraryExt: PluginContext + ContextRandomExt {
    fn get_itineraries(&'_ self, person_id: PersonId) -> EntitySetIterator<'_, Itinerary> {
        self.query_result_iterator::<Itinerary, _>((BelongsTo(person_id),))
    }

    fn get_active_itineraries(&self, person_id: PersonId) -> EntitySetIterator<'_, Itinerary> {
        self.query_result_iterator::<Itinerary, _>((BelongsTo(person_id), Activity(true)))
    }

    fn get_all_settings_for_person(&self, person_id: PersonId) -> Vec<(SettingCategory, f64)> {
        let mut settings = Vec::new();
        for itinerary in self.get_itineraries(person_id) {
            let home_itinerary = self.get_property::<Itinerary, HomeItinerary>(itinerary);
            settings.push((SettingCategory::Home(home_itinerary.home_id), home_itinerary.ratio));
            let workplace_itinerary = self.get_property::<Itinerary, WorkplaceItinerary>(itinerary);
            if let (Some(workplace_id), Some(ratio)) = (workplace_itinerary.workplace_id, workplace_itinerary.ratio) {
                settings.push((SettingCategory::Workplace(workplace_id), ratio));
            }
            let school_itinerary = self.get_property::<Itinerary, SchoolItinerary>(itinerary);
                if let (Some(school_id), Some(ratio)) = (school_itinerary.school_id, school_itinerary.ratio) {
                    settings.push((SettingCategory::School(school_id), ratio));
            }
            let census_tract_itinerary = self.get_property::<Itinerary, CensusTractItinerary>(itinerary);
                settings.push((SettingCategory::CensusTract(census_tract_itinerary.census_tract_id), census_tract_itinerary.ratio));
        }
        settings
    }

    fn get_active_settings_for_person(&self, person_id: PersonId) -> Vec<(SettingCategory, f64)> {
        let mut settings = Vec::new();
        for itinerary in self.get_active_itineraries(person_id) {
            let home_itinerary = self.get_property::<Itinerary, HomeItinerary>(itinerary);
            settings.push((SettingCategory::Home(home_itinerary.home_id), home_itinerary.ratio));
            let workplace_itinerary = self.get_property::<Itinerary, WorkplaceItinerary>(itinerary);
            if let (Some(workplace_id), Some(ratio)) = (workplace_itinerary.workplace_id, workplace_itinerary.ratio) {
                settings.push((SettingCategory::Workplace(workplace_id), ratio));
            }
            let school_itinerary = self.get_property::<Itinerary, SchoolItinerary>(itinerary);
                if let (Some(school_id), Some(ratio)) = (school_itinerary.school_id, school_itinerary.ratio) {
                    settings.push((SettingCategory::School(school_id), ratio));
            }
            let census_tract_itinerary = self.get_property::<Itinerary, CensusTractItinerary>(itinerary);
                settings.push((SettingCategory::CensusTract(census_tract_itinerary.census_tract_id), census_tract_itinerary.ratio));
        }
        settings
    }

    fn get_setting_members(
        &self,
        setting: SettingCategory,
    ) -> Vec<PersonId> {
        match setting {
            SettingCategory::Home(home_id) => {
                let itineraries = self.query_result_iterator::<Itinerary, _>((ItineraryHomeId(home_id),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            },
            SettingCategory::Workplace(workplace_id) => {
                let itineraries = self.query_result_iterator::<Itinerary, _>((ItineraryWorkplaceId(Some(workplace_id)),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            },
            SettingCategory::School(school_id) => {
                let itineraries = self.query_result_iterator::<Itinerary, _>((ItinerarySchoolId(Some(school_id)),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            },
            SettingCategory::CensusTract(census_tract_id) => {
                let itineraries = self.query_result_iterator::<Itinerary, _>((ItineraryCensusTractId(census_tract_id),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            },
        }
    }

    fn sample_setting_member(&mut self, 
        setting: SettingCategory
    ) -> Option<PersonId> {
        match setting {
            SettingCategory::Home(home_id) => {
                self.sample_entity::<Itinerary, _, _>(ItineraryRng, (ItineraryHomeId(home_id), Activity(true),))
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
            },
            SettingCategory::Workplace(workplace_id) => {
                self.sample_entity::<Itinerary, _, _>(ItineraryRng, (ItineraryWorkplaceId(Some(workplace_id)), Activity(true),))
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
            },
            SettingCategory::School(school_id) => {
                self.sample_entity::<Itinerary, _, _>(ItineraryRng, (ItinerarySchoolId(Some(school_id)), Activity(true),))
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
            },
            SettingCategory::CensusTract(census_tract_id) => {
                self.sample_entity::<Itinerary, _, _>(ItineraryRng, (ItineraryCensusTractId(census_tract_id), Activity(true),))
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
            },
        }
    }


}
impl ContextItineraryExt for Context {}