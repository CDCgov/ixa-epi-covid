use ixa::{HashMap, HashMapExt, prelude::*};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::any::Any;
use std::any::TypeId;
use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};
use strum::{EnumCount as EnumCountMacro, EnumIter, IntoEnumIterator};

use crate::itinerary_manager::ContextItineraryModifierExt;
use crate::itinerary_manager::ItineraryModifier;
pub(crate) use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    population_loader::{Itinerary, Person, PersonId},
    setting_code::SettingCode,
};

define_rng!(SettingRng);

#[derive(Clone, Copy)]
enum MembershipSelector {
    Active,
    All,
}

pub enum SettingMembershipChange {
    Active,
    Passive,
    NoChange,
}

pub trait EventListenerStorage: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct EventListener<P: IndexableProperty<Person>> {
    // Use fully qualified syntax for the associated type because type aliases do not have type checking
    event_listener_property_map: HashMap<P, Vec<EventListenerId<PropertyChangeEvent<Person, P>>>>,
}

impl<P: IndexableProperty<Person>> EventListenerStorage for EventListener<P> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
struct EventListenerContainer {
    event_listener_map: HashMap<TypeId, Box<dyn EventListenerStorage>>,
}

#[allow(dead_code)]
impl EventListenerContainer {
    fn get_event_listener_storage<P: IndexableProperty<Person>>(
        &self,
    ) -> Option<&EventListener<P>> {
        let type_id = TypeId::of::<P>();
        self.event_listener_map
            .get(&type_id)
            .and_then(|storage| storage.as_any().downcast_ref::<EventListener<P>>())
    }

    fn get_event_listener_storage_mut<P: IndexableProperty<Person>>(
        &mut self,
    ) -> Option<&mut EventListener<P>> {
        let type_id = TypeId::of::<P>();
        self.event_listener_map
            .get_mut(&type_id)
            .and_then(|storage| storage.as_any_mut().downcast_mut::<EventListener<P>>())
    }

    fn add_event_listener_id<P: IndexableProperty<Person>>(
        &mut self,
        property: P,
        listener_id: EventListenerId<PropertyChangeEvent<Person, P>>,
    ) {
        let type_id = TypeId::of::<P>();
        let storage = self.event_listener_map.entry(type_id).or_insert_with(|| {
            Box::new(EventListener::<P> {
                event_listener_property_map: HashMap::new(),
            })
        });
        let event_listener = storage
            .as_any_mut()
            .downcast_mut::<EventListener<P>>()
            .expect("Failed to downcast to EventListener<P>");
        event_listener
            .event_listener_property_map
            .entry(property)
            .or_default()
            .push(listener_id);
    }

    fn remove_event_listener_ids<P: IndexableProperty<Person>>(
        &mut self,
        property: P,
    ) -> Option<Vec<EventListenerId<PropertyChangeEvent<Person, P>>>> {
        self.get_event_listener_storage_mut::<P>()
            .and_then(|storage| storage.event_listener_property_map.remove(&property))
    }
}

define_data_plugin!(
    EventListenerPlugin,
    EventListenerContainer,
    EventListenerContainer::default()
);

// #[derive(Default)]
// pub struct MembershipEventListeners<P: IndexableProperty<Person>> {
//     event_listeners: HashMap<P, Vec<EventListenerId<PropertyChangeEvent<Person, P>>>>,
// }

// impl<P: IndexableProperty<Person>> MembershipEventListeners<P> {
//     fn add_listener(
//         &mut self,
//         property: P,
//         listener_id: EventListenerId<PropertyChangeEvent<Person, P>>,
//     ) {
//         self.event_listeners
//             .entry(property)
//             .or_default()
//             .push(listener_id);
//     }

//     fn remove_listeners(
//         &mut self,
//         property: P,
//     ) -> Option<Vec<EventListenerId<PropertyChangeEvent<Person, P>>>> {
//         self.event_listeners.remove(&property)
//     }
// }

/// An index of settings as represented by their setting codes.
#[derive(Default)]
pub struct SettingMembership {
    active_members: HashMap<SettingCode, Vec<PersonId>>,
    all_members: HashMap<SettingCode, Vec<PersonId>>,
}

impl SettingMembership {
    fn add_member(&mut self, setting_code: SettingCode, person_id: PersonId) {
        let active_members = self.active_members.entry(setting_code).or_default();
        if !active_members.contains(&person_id) {
            active_members.push(person_id);
        }
        let all_members = self.all_members.entry(setting_code).or_default();
        if !all_members.contains(&person_id) {
            all_members.push(person_id);
        }
    }

    fn add_members(&mut self, setting_codes: &[Option<SettingCode>], person_id: PersonId) {
        for setting_code in setting_codes
            .iter()
            .filter_map(|&setting_code| setting_code)
        {
            self.add_member(setting_code, person_id);
        }
    }

    fn get_members(
        &self,
        setting_code: SettingCode,
        selector: MembershipSelector,
    ) -> Option<&Vec<PersonId>> {
        match selector {
            MembershipSelector::Active => self.active_members.get(&setting_code),
            MembershipSelector::All => self.all_members.get(&setting_code),
        }
    }

    fn remove_member(
        &mut self,
        setting_code: SettingCode,
        person_id: PersonId,
        selector: MembershipSelector,
    ) {
        match selector {
            MembershipSelector::Active => {
                self.active_members
                    .entry(setting_code)
                    .and_modify(|members| members.retain(|id| *id != person_id));
            }
            MembershipSelector::All => {
                self.active_members
                    .entry(setting_code)
                    .and_modify(|members| members.retain(|id| *id != person_id));
                self.all_members
                    .entry(setting_code)
                    .and_modify(|members| members.retain(|id| *id != person_id));
            }
        }
    }

    fn member_count(&self, setting_code: SettingCode, selector: MembershipSelector) -> usize {
        match selector {
            MembershipSelector::Active => self
                .active_members
                .get(&setting_code)
                .map(|members| members.len())
                .unwrap_or(0),
            MembershipSelector::All => self
                .all_members
                .get(&setting_code)
                .map(|members| members.len())
                .unwrap_or(0),
        }
    }
}

define_data_plugin!(SettingMembershipPlugin, SettingMembership, |context| {
    let mut membership = SettingMembership::default();
    let person_iter = context.get_entity_iterator::<Person>();

    for person_id in person_iter {
        let itinerary: Itinerary = context.get_property(person_id);
        membership.add_members(&itinerary.setting_ids, person_id);
    }

    membership
});

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

impl<T> Index<SettingCategory> for [T; SETTING_COUNT] {
    type Output = T;
    fn index(&self, index: SettingCategory) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<SettingCategory> for [T; SETTING_COUNT] {
    fn index_mut(&mut self, index: SettingCategory) -> &mut Self::Output {
        &mut self[index as usize]
    }
}

pub const SETTING_COUNT: usize = SettingCategory::COUNT;

define_global_property!(SettingAlphas, [f64; SETTING_COUNT]);
define_global_property!(SettingRatios, [f64; SETTING_COUNT]);

trait ContextSettingExtPrivate:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextItineraryModifierExt
{
    #[allow(dead_code)]
    fn get_setting_ratio(&self, setting_category: SettingCategory) -> Result<f64, ModelError> {
        let ratios = self.get_global_property_value(SettingRatios).unwrap();
        Ok(ratios[setting_category])
    }

    fn get_setting_alpha(&self, setting_category: SettingCategory) -> Result<f64, ModelError> {
        let alphas = self.get_global_property_value(SettingAlphas).unwrap();
        Ok(alphas[setting_category])
    }

    fn get_setting_size_internal(
        &self,
        setting: SettingCode,
        selector: MembershipSelector,
    ) -> usize {
        let membership = self.get_data(SettingMembershipPlugin);
        membership.member_count(setting, selector)
    }

    #[allow(clippy::type_complexity)]
    fn get_settings_for_person_internal(
        &self,
        person_id: PersonId,
        selector: MembershipSelector,
    ) -> Result<SmallVec<[(SettingCode, f64, f64); SETTING_COUNT]>, ModelError> {
        let mut active_settings = SmallVec::<[(SettingCode, f64, f64); SETTING_COUNT]>::new();
        let setting_ids = self
            .get_property::<Person, Itinerary>(person_id)
            .setting_ids;
        let itinerary_ratios = match selector {
            MembershipSelector::Active => self.get_itinerary(person_id),
            MembershipSelector::All => {
                self.get_property::<Person, Itinerary>(person_id)
                    .itinerary_ratios
            }
        };

        for category in SettingCategory::iter() {
            if let Some(id) = setting_ids[category] {
                let ratio = itinerary_ratios[category];
                if ratio > 0.0 {
                    let multiplier = self.calculate_multiplier(id, selector)?;
                    active_settings.push((id, ratio, multiplier));
                }
            }
        }
        Ok(active_settings)
    }

    fn calculate_multiplier(
        &self,
        setting: SettingCode,
        selector: MembershipSelector,
    ) -> Result<f64, ModelError> {
        let alpha = self.get_setting_alpha(setting.category())?;
        match alpha {
            0.0 => {
                // (n-1)^0 = 1
                Ok(1.0)
            }
            1.0 => {
                let size = self.get_setting_size_internal(setting, selector);
                Ok((size - 1) as f64)
            }
            alpha => {
                let size = self.get_setting_size_internal(setting, selector);
                let size = (size - 1) as f64;
                Ok(size.powf(alpha))
            }
        }
    }
}
impl ContextSettingExtPrivate for Context {}

#[allow(private_bounds)]
pub trait ContextSettingExt:
    PluginContext
    + ContextEntitiesExt
    + ContextSettingExtPrivate
    + ContextParametersExt
    + ContextItineraryModifierExt
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

    fn get_setting_size(&self, setting: SettingCode) -> usize {
        let membership = self.get_data(SettingMembershipPlugin);
        membership.member_count(setting, MembershipSelector::Active)
    }

    #[allow(clippy::type_complexity)]
    fn get_settings_for_person(
        &self,
        person_id: PersonId,
    ) -> Result<SmallVec<[(SettingCode, f64, f64); SETTING_COUNT]>, ModelError> {
        self.get_settings_for_person_internal(person_id, MembershipSelector::Active)
    }

    fn calculate_current_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        // active settings is a vector of (setting_id, ratio, multiplier) for the person.
        // When iterating through this vector s.0 refers to the setting_id, s.1 refers to ratio of time the person spends in the setting
        // and s.2 refers to the multiplier for that setting based on its size and alpha.
        let active_settings = self
            .get_settings_for_person_internal(person_id, MembershipSelector::Active)
            .unwrap();
        let mut current_inf = 0.0;
        let mut sum_ratio = 0.0;
        // we calculate sum of ratios to normalize weights
        for (_, ratio, _) in active_settings.iter() {
            sum_ratio += ratio;
        }
        // Current infectiousness multiplier is the weighted average of the setting specific multipliers
        // where weights are given by the ratio of time spent in each setting normalized by the sum of ratios
        // across active settings.
        if sum_ratio > 0.0 {
            for (_, ratio, multiplier) in active_settings.iter() {
                current_inf += multiplier * (ratio / sum_ratio);
            }
        }
        current_inf
    }

    fn calculate_max_infectiousness_multiplier_for_person(&self, person_id: PersonId) -> f64 {
        // active settings is a vector of (setting_id, ratio, multiplier) for the person.
        // When iterating through this vector s.0 refers to the setting_id, s.1 refers to ratio of time the person spends in the setting
        // and s.2 refers to the multiplier for that setting based on its size and alpha.
        let active_settings = self
            .get_settings_for_person_internal(person_id, MembershipSelector::All)
            .unwrap();
        let mut max_inf = 0.0;
        for (_, _, multiplier) in active_settings.iter() {
            max_inf = f64::max(max_inf, *multiplier);
        }
        max_inf
    }

    fn sample_person_from_setting(&self, setting: SettingCode) -> Result<PersonId, ModelError> {
        let membership = self.get_data(SettingMembershipPlugin);
        let members = membership
            .get_members(setting, MembershipSelector::Active)
            .ok_or_else(|| {
                ModelError::ModelError(format!("No members found for setting: {:?}", setting))
            })?;
        let idx = self.sample_range(SettingRng, 0..members.len());
        Ok(members[idx])
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: PersonId,
        setting: SettingCode,
    ) -> Result<Option<PersonId>, ModelError> {
        if self.get_setting_size_internal(setting, MembershipSelector::Active) == 1 {
            return Ok(None);
        }
        loop {
            let sampled_person = self.sample_person_from_setting(setting)?;
            if sampled_person != person_id {
                return Ok(Some(sampled_person));
            }
        }
    }

    fn sample_active_setting(&self, person_id: PersonId) -> Result<SettingCode, ModelError> {
        let active_settings = self
            .get_settings_for_person_internal(person_id, MembershipSelector::Active)
            .unwrap();
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

    fn add_person_to_settings(
        &mut self,
        person_id: PersonId,
        home_id: Option<SettingCode>,
        work_id: Option<SettingCode>,
        school_id: Option<SettingCode>,
        community_id: Option<SettingCode>,
    ) {
        let setting_ids = [
            // Keep this ordering in sync with the SettingCategory enum, which is used to index into
            // this array.
            home_id,
            work_id,
            school_id,
            community_id,
        ];

        // The default itinerary ratios.
        let default_itinerary_ratios = *self.get_global_property_value(SettingRatios).unwrap();

        let itinerary_ratios = std::array::from_fn(|i| {
            if setting_ids[i].is_some() {
                default_itinerary_ratios[i]
            } else {
                0.0
            }
        });

        let sum_ratio = itinerary_ratios.iter().copied().sum::<f64>();

        let normalized_itinerary_ratios = itinerary_ratios.map(|ratio| ratio / sum_ratio);

        self.set_property::<Person, Itinerary>(
            person_id,
            Itinerary {
                setting_ids,
                itinerary_ratios: normalized_itinerary_ratios,
            },
        );
        // Register setting membership for this person.
        let membership = self.get_data_mut(SettingMembershipPlugin);
        membership.add_members(&setting_ids, person_id);
    }

    fn add_person_to_single_setting(&mut self, person: PersonId, setting_id: SettingCode) {
        let data = self.get_data_mut(SettingMembershipPlugin);
        data.add_member(setting_id, person);
    }

    fn remove_person_from_setting(&mut self, person_id: PersonId, setting_id: SettingCode) {
        let membership = self.get_data_mut(SettingMembershipPlugin);
        membership.remove_member(setting_id, person_id, MembershipSelector::Active);
    }
    fn setup_itinerary_modifer<P: IndexableProperty<Person>, I: ItineraryModifier>(
        &mut self,
        person_property: P,
        modifier: I,
        setting_membership_change: SettingMembershipChange,
    ) {
        self.register_itinerary_modifier(person_property, modifier);
        match setting_membership_change {
            SettingMembershipChange::Active => {
                self.handle_active_setting_membership_change(person_property, false);
            }
            SettingMembershipChange::Passive => {
                self.setup_event_subscription_for_setting_membership(person_property);
            }
            SettingMembershipChange::NoChange => (),
        }
    }

    fn remove_itinerary_modifier<P: IndexableProperty<Person>>(&mut self, person_property: P) {
        self.remove_itinerary_modifier_by_property(person_property);
        let data = self.get_data_mut(EventListenerPlugin);
        if let Some(listener_ids) = data.remove_event_listener_ids(person_property) {
            for listener_id in listener_ids {
                self.unsubscribe_from_event::<PropertyChangeEvent<Person, P>>(&listener_id);
            }
        }
        self.handle_active_setting_membership_change(person_property, true);
    }

    /// Subscribe to a general person property event for itinerary modifier registration.
    fn setup_event_subscription_for_setting_membership<P: IndexableProperty<Person>>(
        &mut self,
        person_property: P,
    ) {
        let listener: EventListenerId<PropertyChangeEvent<Person, P>> = self
            .subscribe_to_event::<PropertyChangeEvent<Person, P>>(move |context, event| {
                if event.current == person_property {
                    context
                        .determine_setting_membership_changes(event.entity_id)
                        .into_iter()
                        .filter_map(|(setting_id, remove)| remove.then_some(setting_id))
                        .for_each(|setting_id| {
                            context.remove_person_from_setting(event.entity_id, setting_id);
                        });
                }
                if event.previous == person_property {
                    context
                        .determine_setting_membership_changes(event.entity_id)
                        .into_iter()
                        .filter_map(|(setting_id, add)| add.then_some(setting_id))
                        .for_each(|setting_id| {
                            context.add_person_to_single_setting(event.entity_id, setting_id);
                        });
                }
            });
        let data = self.get_data_mut(EventListenerPlugin);
        data.add_event_listener_id(person_property, listener);
    }

    fn handle_active_setting_membership_change<P>(
        &mut self,
        person_property: P,
        add_to_settings: bool,
    ) where
        P: IndexableProperty<Person>,
    {
        let remaining_people: Vec<_> = self
            .query_result_iterator(with!(Person, person_property))
            .collect();
        for person_id in remaining_people {
            self.determine_setting_membership_changes(person_id)
                .into_iter()
                .filter_map(|(setting_id, remove)| remove.then_some(setting_id))
                .for_each(|setting_id| {
                    if add_to_settings {
                        self.add_person_to_single_setting(person_id, setting_id);
                    } else {
                        self.remove_person_from_setting(person_id, setting_id);
                    }
                });
        }
    }

    fn determine_setting_membership_changes(
        &self,
        person_id: PersonId,
    ) -> Vec<(SettingCode, bool)> {
        let default_itinerary = self
            .get_property::<Person, Itinerary>(person_id)
            .itinerary_ratios;
        let modified_itinerary = self.get_itinerary(person_id);
        let mut changes = Vec::new();
        let setting_ids = self
            .get_property::<Person, Itinerary>(person_id)
            .setting_ids;
        for i in 0..SETTING_COUNT {
            if modified_itinerary[i] == 0.0
                && default_itinerary[i] > 0.0
                && let Some(setting_id) = setting_ids[i]
            {
                changes.push((setting_id, true));
            }
        }
        changes
    }
}

impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.register_setting_global_properties();
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::itinerary_modifiers::create_itinerary_transition_matrix;
    use crate::population_loader::{CommunityId, HomeId, SchoolId, WorkId};
    use crate::{
        Age, Params,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::{
            FIPSCode, PopulationReaderSettingCategory,
            parser::{parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id},
        },
    };
    use ixa::{HashMap, assert_almost_eq};

    fn make_home_id(home_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_home_id(home_id).unwrap().1)
    }

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn make_workplace_id(workplace_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_workplace_id(workplace_id).unwrap().1)
    }

    fn make_community_id(home_id: &[u8]) -> SettingCode {
        let home_id = make_home_id(home_id).0;
        SettingCode(
            FIPSCode::with_category(
                home_id.state_code(),
                home_id.county_code(),
                home_id.census_tract_code(),
                PopulationReaderSettingCategory::CensusTract.encode(),
            )
            .unwrap(),
        )
    }

    fn setup_test_context(alpha: f64) -> Context {
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
        assignments: &[SettingCode],
        itinerary_ratios: [f64; SETTING_COUNT],
    ) {
        let mut setting_ids = [None; SETTING_COUNT];
        for setting_code in assignments {
            setting_ids[setting_code.category()] = Some(*setting_code);
        }
        context.set_property::<Person, Itinerary>(
            person_id,
            Itinerary {
                setting_ids,
                itinerary_ratios,
            },
        );
    }

    #[test]
    fn test_get_setting_size_empty() {
        let context = setup_test_context(0.0);
        let home_id = SettingCode::arbitrary_home_code();
        // No persons assigned yet
        let size = context.get_setting_size(home_id);
        assert_eq!(size, 0);
    }

    #[test]
    fn test_get_setting_size_multiple_categories() {
        let mut context = setup_test_context(0.0);
        let home_code = SettingCode::arbitrary_home_code();
        let community_code = home_code.extract_community();
        let work_code = SettingCode::arbitrary_workplace_code();

        let person1 = context.add_entity(with!(Person, Age(20))).unwrap();
        let person2 = context.add_entity(with!(Person, Age(21))).unwrap();
        let person3 = context.add_entity(with!(Person, Age(22))).unwrap();
        context.add_person_to_settings(person1, Some(home_code), None, None, Some(community_code));
        context.add_person_to_settings(person2, Some(home_code), None, None, Some(community_code));
        context.add_person_to_settings(person3, None, Some(work_code), None, None);

        let home_id = context.get_property::<Person, HomeId>(person1).0.unwrap();
        let work_id = context.get_property::<Person, WorkId>(person3).0.unwrap();
        let home_size = context.get_setting_size(home_id);
        let work_size = context.get_setting_size(work_id);
        assert_eq!(home_size, 2);
        assert_eq!(work_size, 1);
    }

    #[test]
    fn test_sample_person_from_home() {
        let mut context = setup_test_context(0.0);
        let person_id = context.add_entity(with!(Person, Age(20))).unwrap();
        context.add_person_to_settings(
            person_id,
            Some(make_home_id(b"160379602000011")),
            None,
            None,
            None,
        );
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let sampled = context.sample_person_from_setting(home_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_work() {
        let mut context = setup_test_context(0.0);
        let person_id = context.add_entity(with!(Person, Age(30))).unwrap();
        context.add_person_to_settings(
            person_id,
            None,
            Some(make_workplace_id(b"1603796020001332")),
            None,
            None,
        );
        let work_id = context.get_property::<Person, WorkId>(person_id).0.unwrap();
        let sampled = context.sample_person_from_setting(work_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_school() {
        let mut context = setup_test_context(0.0);
        let person_id = context.add_entity(with!(Person, Age(10))).unwrap();
        context.add_person_to_settings(
            person_id,
            None,
            None,
            Some(make_school_id(b"16037960200002")),
            None,
        );
        let school_id = context
            .get_property::<Person, SchoolId>(person_id)
            .0
            .unwrap();
        let sampled = context.sample_person_from_setting(school_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_person_from_community() {
        let mut context = setup_test_context(0.0);
        let person_id = context.add_entity(with!(Person, Age(40))).unwrap();
        context.add_person_to_settings(
            person_id,
            None,
            None,
            None,
            Some(make_community_id(b"160379602000011")),
        );
        let community_id = context
            .get_property::<Person, CommunityId>(person_id)
            .0
            .unwrap();
        let sampled = context.sample_person_from_setting(community_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_get_setting_size() {
        let mut context = setup_test_context(0.0);
        let home_id = SettingCode::arbitrary_home_code();
        let person1 = context.add_entity(with!(Person, Age(20))).unwrap();
        let person2 = context.add_entity(with!(Person, Age(21))).unwrap();
        assign_person_settings(&mut context, person1, &[home_id], [1.0, 0.0, 0.0, 0.0]);
        assign_person_settings(&mut context, person2, &[home_id], [1.0, 0.0, 0.0, 0.0]);
        let size = context.get_setting_size(home_id);
        assert_eq!(size, 2);
    }

    #[test]
    fn test_get_setting_ratio() {
        let context = setup_test_context(0.0);
        let ratio = context.get_setting_ratio(SettingCategory::School).unwrap();
        assert_eq!(ratio, 0.25);
    }

    #[test]
    fn test_get_active_settings_for_person() {
        let mut context = setup_test_context(0.5);
        let home_id = SettingCode::arbitrary_home_code();
        let person_id = context.add_entity(with!(Person, Age(22))).unwrap();
        assign_person_settings(&mut context, person_id, &[home_id], [0.25, 0.0, 0.0, 0.0]);
        let active = context.get_settings_for_person(person_id).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, home_id);
        assert_eq!(active[0].1, 0.25); // default ratio for home is 0.25
        assert_eq!(active[0].2, 0.0); // alpha is set to 0.5 size is set to 1 so the multipler is (1-1)^0.5 = 0
    }

    #[test]
    fn test_calculate_current_infectiousness_multiplier_for_person() {
        let alpha = 0.5;
        let mut context = setup_test_context(alpha);
        let home_id = SettingCode::arbitrary_home_code();
        let work_id = home_id.as_arbitrary_workplace_code();
        let p1 = context.add_entity(with!(Person, Age(23))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(23))).unwrap();
        let p3 = context.add_entity(with!(Person, Age(23))).unwrap();
        assign_person_settings(&mut context, p1, &[home_id, work_id], [0.5, 0.5, 0.0, 0.0]);
        assign_person_settings(&mut context, p2, &[home_id, work_id], [0.5, 0.5, 0.0, 0.0]);
        assign_person_settings(&mut context, p3, &[home_id], [1.0, 0.0, 0.0, 0.0]);

        let val = context.calculate_current_infectiousness_multiplier_for_person(p1);
        // home size = 3, alpha = 0.5, so multiplier = (3-1)^0.5 = 2^0.5 = 1.41 * 0.5 = 0.707
        // work size = 2, alpha = 0.5, so multiplier = (2-1)^0.5 = 1^0.5 = 1 * 0.5 = 0.5
        assert_almost_eq!(val, 1.207, 0.001);
    }

    #[test]
    fn test_calculate_max_infectiousness_multiplier_for_person() {
        let alpha = 1.0;
        let mut context = setup_test_context(alpha);
        let home_id = SettingCode::arbitrary_home_code();
        let work_id = home_id.as_arbitrary_workplace_code();
        let p1 = context.add_entity(with!(Person, Age(24))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(24))).unwrap();
        assign_person_settings(&mut context, p1, &[home_id, work_id], [0.5, 0.5, 0.0, 0.0]);
        assign_person_settings(&mut context, p2, &[home_id], [1.0, 0.0, 0.0, 0.0]);
        let val = context.calculate_max_infectiousness_multiplier_for_person(p1);
        // size = 2, alpha = 1.0, so multiplier = (2-1)^1 = 1
        assert_eq!(val, 1.0);
    }

    #[test]
    fn test_sample_person_from_setting() {
        let mut context = setup_test_context(0.0);
        let comm_id = SettingCode::arbitrary_home_code().extract_community();
        let person_id = context.add_entity(with!(Person, Age(25))).unwrap();
        assign_person_settings(&mut context, person_id, &[comm_id], [0.0, 0.0, 0.0, 1.0]);
        let sampled = context.sample_person_from_setting(comm_id).unwrap();
        assert_eq!(sampled, person_id);
    }

    #[test]
    fn test_sample_from_setting_with_exclusion() {
        let mut context = setup_test_context(0.0);
        let work_id = SettingCode::arbitrary_workplace_code();
        let p1 = context.add_entity(with!(Person, Age(26))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(27))).unwrap();
        assign_person_settings(&mut context, p1, &[work_id], [0.0, 1.0, 0.0, 0.0]);
        assign_person_settings(&mut context, p2, &[work_id], [0.0, 1.0, 0.0, 0.0]);
        let sampled = context
            .sample_from_setting_with_exclusion(p1, work_id)
            .unwrap();
        assert_eq!(sampled, Some(p2));
    }

    #[test]
    fn test_sample_active_setting() {
        let mut context = setup_test_context(0.0);
        let home_id = SettingCode::arbitrary_home_code();
        let person_id = context.add_entity(with!(Person, Age(30))).unwrap();
        assign_person_settings(&mut context, person_id, &[home_id], [1.0, 0.0, 0.0, 0.0]);
        let sampled = context.sample_active_setting(person_id).unwrap();
        assert_eq!(sampled, home_id);
    }

    #[test]
    fn test_add_person_to_setting_and_add_index_setting() {
        let mut context = setup_test_context(0.0);
        let person_id = context.add_entity(with!(Person, Age(31))).unwrap();
        let setting_code = make_home_id(b"160379602000010");
        context.add_person_to_settings(person_id, Some(setting_code), None, None, None);
        let home_id = context.get_property::<Person, HomeId>(person_id).0.unwrap();
        let itinerary = context.get_property::<Person, Itinerary>(person_id);
        let setting_ids = itinerary.setting_ids;
        let itinerary_ratios = itinerary.itinerary_ratios;
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

    #[test]
    fn test_active_settings_with_itinerary_modifiers() {
        let mut context = setup_test_context(0.0);
        let person_id = context.add_entity(with!(Person, Age(20))).unwrap();
        context.add_person_to_settings(
            person_id,
            Some(make_home_id(b"160379602000011")),
            Some(make_workplace_id(b"1603796020001332")),
            Some(make_school_id(b"1603796020001443")),
            Some(make_community_id(b"160379602000011")),
        );
        let active_settings = context.get_settings_for_person(person_id).unwrap();
        let setting_codes: Vec<SettingCode> = active_settings.iter().map(|s| s.0).collect();
        let itinerary_ratios: Vec<f64> = active_settings.iter().map(|s| s.1).collect();
        let multipliers: Vec<f64> = active_settings.iter().map(|s| s.2).collect();

        let expected_setting_codes = vec![
            make_home_id(b"160379602000011"),
            make_workplace_id(b"1603796020001332"),
            make_school_id(b"1603796020001443"),
            make_community_id(b"160379602000011"),
        ];

        let expected_itinerary_ratios = vec![0.25, 0.25, 0.25, 0.25];

        let expected_multipliers = vec![1.0, 1.0, 1.0, 1.0]; // alpha is set to 0.0 so all multipliers are 1 regardless of size

        assert_eq!(setting_codes, expected_setting_codes);
        assert_eq!(itinerary_ratios, expected_itinerary_ratios);
        assert_eq!(multipliers, expected_multipliers);

        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.0, 0.0, 0.5],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let weekend_modifier = create_itinerary_transition_matrix(Some(weekend_matrix), None, None);

        context.register_itinerary_modifier(Age(20), weekend_modifier);

        let active_settings = context.get_settings_for_person(person_id).unwrap();
        let setting_codes: Vec<SettingCode> = active_settings.iter().map(|s| s.0).collect();
        let itinerary_ratios: Vec<f64> = active_settings.iter().map(|s| s.1).collect();
        let multipliers: Vec<f64> = active_settings.iter().map(|s| s.2).collect();

        let expected_setting_codes = vec![
            make_home_id(b"160379602000011"),
            make_community_id(b"160379602000011"),
        ];
        let expected_itinerary_ratios = vec![0.5, 0.5];
        let expected_multipliers = vec![1.0, 1.0]; // alpha is set to 0.0 so all multipliers are 1 regardless of size

        assert_eq!(setting_codes, expected_setting_codes);
        assert_eq!(itinerary_ratios, expected_itinerary_ratios);
        assert_eq!(multipliers, expected_multipliers);
    }

    #[test]
    fn test_setup_itinerary_modifier_and_remove() {
        let mut context = setup_test_context(0.0);
        let p1 = context.add_entity(with!(Person, Age(20))).unwrap();
        let home_id = make_home_id(b"160379602000011");
        let work_id = make_workplace_id(b"1603796020001332");
        let school_id = make_school_id(b"1603796020001443");
        let community_id = make_community_id(b"160379602000011");
        context.add_person_to_settings(
            p1,
            Some(home_id),
            Some(work_id),
            Some(school_id),
            Some(community_id),
        );

        let p2 = context.add_entity(with!(Person, Age(21))).unwrap();
        context.add_person_to_settings(
            p2,
            Some(home_id),
            Some(work_id),
            Some(school_id),
            Some(community_id),
        );

        let weekend_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ];

        let weekend_modifier = create_itinerary_transition_matrix(Some(weekend_matrix), None, None);

        context.setup_itinerary_modifer(Age(20), weekend_modifier, SettingMembershipChange::Active);

        let active_settings = context.get_settings_for_person(p1).unwrap();
        assert_eq!(active_settings.len(), 2); // Only home and community should be active
        assert_eq!(context.get_setting_size(home_id), 2); // Both p1 and p2 are still members of home
        assert_eq!(context.get_setting_size(work_id), 1); // Only p2 is a member of work now
        assert_eq!(context.get_setting_size(school_id), 1); // Only p2 is a member of school now
        assert_eq!(context.get_setting_size(community_id), 2); // Both p1 and p2 are still members of community

        context.remove_itinerary_modifier(Age(20));

        let active_settings_after_removal = context.get_settings_for_person(p1).unwrap();
        assert_eq!(active_settings_after_removal.len(), 4); // All settings should be active again
        println!(
            "Active settings for p1: {:?}",
            active_settings_after_removal
        );
        assert_eq!(context.get_setting_size(home_id), 2); // Both p1 and p2 are still members of home
        assert_eq!(context.get_setting_size(work_id), 2); // Only p2 is a member of work now
        assert_eq!(context.get_setting_size(school_id), 2); // Only p2 is a member of school now
        assert_eq!(context.get_setting_size(community_id), 2); // Both p1 and p2 are still members of community
    }
}
