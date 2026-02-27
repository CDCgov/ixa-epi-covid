use ixa::{entity::EntitySetIterator, impl_derived_property, prelude::*};

use serde::{Deserialize, Serialize};

use crate::{
    population_loader::PersonId,
    settings_entities::{Setting, SettingCategory, SettingId},
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
    pub home_id: Option<SettingId>,
    pub ratio: Option<f64>,
}
impl_property!(HomeItinerary, Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct WorkplaceItinerary {
    pub workplace_id: Option<SettingId>,
    pub ratio: Option<f64>,
}
impl_property!(WorkplaceItinerary, Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct SchoolItinerary {
    pub school_id: Option<SettingId>,
    pub ratio: Option<f64>,
}
impl_property!(SchoolItinerary, Itinerary);
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct CensusTractItinerary {
    pub census_tract_id: Option<SettingId>,
    pub ratio: Option<f64>,
}
impl_property!(CensusTractItinerary, Itinerary);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct HomeId(pub Option<SettingId>);
impl_derived_property!(HomeId, Itinerary, [HomeItinerary], [], |home_itinerary| {
    HomeId(home_itinerary.home_id)
});

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct SchoolId(pub Option<SettingId>);
impl_derived_property!(
    SchoolId,
    Itinerary,
    [SchoolItinerary],
    [],
    |school_itinerary| SchoolId(school_itinerary.school_id)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct WorkplaceId(pub Option<SettingId>);
impl_derived_property!(
    WorkplaceId,
    Itinerary,
    [WorkplaceItinerary],
    [],
    |workplace_itinerary| WorkplaceId(workplace_itinerary.workplace_id)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub struct CensusTractId(pub Option<SettingId>);
impl_derived_property!(
    CensusTractId,
    Itinerary,
    [CensusTractItinerary],
    [],
    |census_tract_itinerary| CensusTractId(census_tract_itinerary.census_tract_id)
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum ItineraryUse {
    Default,
    Isolated,
    Hospitalized,
    Quarantined,
}
impl_property!(
    ItineraryUse,
    Itinerary,
    default_const = ItineraryUse::Default
);

pub trait ContextItineraryExt: PluginContext + ContextRandomExt {
    fn get_itineraries(&'_ self, person_id: PersonId) -> EntitySetIterator<'_, Itinerary> {
        self.query_result_iterator::<Itinerary, _>((BelongsTo(person_id),))
    }

    fn get_active_itineraries(&self, person_id: PersonId) -> EntitySetIterator<'_, Itinerary> {
        self.query_result_iterator::<Itinerary, _>((BelongsTo(person_id), Activity(true)))
    }

    fn get_all_settings_for_person(&self, person_id: PersonId) -> Vec<(SettingId, f64)> {
        let mut settings = Vec::new();
        for itinerary in self.get_itineraries(person_id) {
            let home_itinerary = self.get_property::<Itinerary, HomeItinerary>(itinerary);
            if let (Some(home_id), Some(ratio)) = (home_itinerary.home_id, home_itinerary.ratio) {
                settings.push((home_id, ratio));
            }
            let workplace_itinerary = self.get_property::<Itinerary, WorkplaceItinerary>(itinerary);
            if let (Some(workplace_id), Some(ratio)) =
                (workplace_itinerary.workplace_id, workplace_itinerary.ratio)
            {
                settings.push((workplace_id, ratio));
            }
            let school_itinerary = self.get_property::<Itinerary, SchoolItinerary>(itinerary);
            if let (Some(school_id), Some(ratio)) =
                (school_itinerary.school_id, school_itinerary.ratio)
            {
                settings.push((school_id, ratio));
            }
            let census_tract_itinerary =
                self.get_property::<Itinerary, CensusTractItinerary>(itinerary);
            if let (Some(census_tract_id), Some(ratio)) = (
                census_tract_itinerary.census_tract_id,
                census_tract_itinerary.ratio,
            ) {
                settings.push((census_tract_id, ratio));
            }
        }
        settings
    }

    fn get_active_settings_for_person(&self, person_id: PersonId) -> Vec<(SettingId, f64)> {
        let mut settings = Vec::new();
        for itinerary in self.get_active_itineraries(person_id) {
            let home_itinerary = self.get_property::<Itinerary, HomeItinerary>(itinerary);
            if let (Some(home_id), Some(ratio)) = (home_itinerary.home_id, home_itinerary.ratio) {
                settings.push((home_id, ratio));
            }
            let workplace_itinerary = self.get_property::<Itinerary, WorkplaceItinerary>(itinerary);
            if let (Some(workplace_id), Some(ratio)) =
                (workplace_itinerary.workplace_id, workplace_itinerary.ratio)
            {
                settings.push((workplace_id, ratio));
            }
            let school_itinerary = self.get_property::<Itinerary, SchoolItinerary>(itinerary);
            if let (Some(school_id), Some(ratio)) =
                (school_itinerary.school_id, school_itinerary.ratio)
            {
                settings.push((school_id, ratio));
            }
            let census_tract_itinerary =
                self.get_property::<Itinerary, CensusTractItinerary>(itinerary);
            if let (Some(census_tract_id), Some(ratio)) = (
                census_tract_itinerary.census_tract_id,
                census_tract_itinerary.ratio,
            ) {
                settings.push((census_tract_id, ratio));
            }
        }
        settings
    }

    fn get_setting_members(&self, setting: SettingId) -> Vec<PersonId> {
        let setting_category =
            self.get_property::<Setting, crate::settings_entities::SettingCategory>(setting);
        match setting_category {
            SettingCategory::Home => {
                let itineraries =
                    self.query_result_iterator::<Itinerary, _>((HomeId(Some(setting)),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            }
            SettingCategory::Workplace => {
                let itineraries =
                    self.query_result_iterator::<Itinerary, _>((WorkplaceId(Some(setting)),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            }
            SettingCategory::School => {
                let itineraries =
                    self.query_result_iterator::<Itinerary, _>((SchoolId(Some(setting)),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            }
            SettingCategory::CensusTract => {
                let itineraries =
                    self.query_result_iterator::<Itinerary, _>((CensusTractId(Some(setting)),));
                itineraries
                    .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0)
                    .collect::<Vec<_>>()
            }
        }
    }

    fn sample_setting_member(&mut self, setting: SettingId) -> Option<PersonId> {
        let setting_category =
            self.get_property::<Setting, crate::settings_entities::SettingCategory>(setting);
        match setting_category {
            SettingCategory::Home => self
                .sample_entity::<Itinerary, _, _>(
                    ItineraryRng,
                    (HomeId(Some(setting)), Activity(true)),
                )
                .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0),
            SettingCategory::Workplace => self
                .sample_entity::<Itinerary, _, _>(
                    ItineraryRng,
                    (WorkplaceId(Some(setting)), Activity(true)),
                )
                .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0),
            SettingCategory::School => self
                .sample_entity::<Itinerary, _, _>(
                    ItineraryRng,
                    (SchoolId(Some(setting)), Activity(true)),
                )
                .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0),
            SettingCategory::CensusTract => self
                .sample_entity::<Itinerary, _, _>(
                    ItineraryRng,
                    (CensusTractId(Some(setting)), Activity(true)),
                )
                .map(|itinerary| self.get_property::<Itinerary, BelongsTo>(itinerary).0),
        }
    }

    fn normalize_itinerary_ratios(&mut self, person_id: PersonId) {
        let itineraries = self.get_itineraries(person_id).collect::<Vec<_>>();
        let mut total_ratio = 0.0;
        for itinerary in &itineraries {
            for ratio in [
                self.get_property::<Itinerary, HomeItinerary>(*itinerary)
                    .ratio,
                self.get_property::<Itinerary, WorkplaceItinerary>(*itinerary)
                    .ratio,
                self.get_property::<Itinerary, SchoolItinerary>(*itinerary)
                    .ratio,
                self.get_property::<Itinerary, CensusTractItinerary>(*itinerary)
                    .ratio,
            ]
            .into_iter()
            .flatten()
            {
                total_ratio += ratio;
            }
        }

        if total_ratio > 0.0 {
            for itinerary in itineraries {
                let mut home_itinerary = self.get_property::<Itinerary, HomeItinerary>(itinerary);
                if let Some(ratio) = home_itinerary.ratio {
                    home_itinerary.ratio = Some(ratio / total_ratio);
                    self.set_property(itinerary, home_itinerary);
                }
                let mut workplace_itinerary =
                    self.get_property::<Itinerary, WorkplaceItinerary>(itinerary);
                if let Some(ratio) = workplace_itinerary.ratio {
                    workplace_itinerary.ratio = Some(ratio / total_ratio);
                    self.set_property(itinerary, workplace_itinerary);
                }
                let mut school_itinerary =
                    self.get_property::<Itinerary, SchoolItinerary>(itinerary);
                if let Some(ratio) = school_itinerary.ratio {
                    school_itinerary.ratio = Some(ratio / total_ratio);
                    self.set_property(itinerary, school_itinerary);
                }
                let mut census_tract_itinerary =
                    self.get_property::<Itinerary, CensusTractItinerary>(itinerary);
                if let Some(ratio) = census_tract_itinerary.ratio {
                    census_tract_itinerary.ratio = Some(ratio / total_ratio);
                    self.set_property(itinerary, census_tract_itinerary);
                }
            }
        }
    }
}
impl ContextItineraryExt for Context {}
