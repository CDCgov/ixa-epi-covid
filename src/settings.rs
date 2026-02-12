use crate::{
    parameters::{ContextParametersExt, CoreSettingsTypes, Params},
    population_loader::Person,
};

use indexmap::set::IndexSet;
use ixa::prelude::*;
use ixa::{
    Context, ContextEntitiesExt, ContextRandomExt, HashMap, HashMapExt, HashSet, IxaError,
    PluginContext, define_data_plugin, define_rng, profiling::open_span,
};
use serde::{Deserialize, Serialize};

use std::{any::TypeId, hash::Hash};

use dyn_clone::DynClone;

define_rng!(SettingsRng);

// This is not the most flexible structure but would work for now
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct SettingProperties {
    pub alpha: f64,
}

pub trait SettingCategory: std::fmt::Debug + 'static {
    fn get_type_id(&self) -> std::any::TypeId;
}

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub struct SettingId<T: SettingCategory> {
    pub id: usize,
    // Marker to say this group id is associated with T (but does not own it)
    _phantom: std::marker::PhantomData<T>,
}

pub trait AnySettingId
where
    Self: std::fmt::Debug + DynClone + 'static,
{
    fn id(&self) -> usize;
    fn calculate_multiplier(
        &self,
        members: &IndexSet<EntityId<Person>>,
        setting_properties: SettingProperties,
    ) -> f64;
    fn get_category_id(&self) -> &'static str;
    fn get_type_id(&self) -> TypeId;
    fn get_tuple_id(&self) -> (TypeId, usize) {
        (self.get_type_id(), self.id())
    }
}

dyn_clone::clone_trait_object!(AnySettingId);

impl<T: SettingCategory + Clone> AnySettingId for SettingId<T> {
    fn id(&self) -> usize {
        self.id
    }
    fn get_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
    #[allow(clippy::cast_precision_loss)]
    fn calculate_multiplier(
        &self,
        members: &IndexSet<EntityId<Person>>,
        setting_properties: SettingProperties,
    ) -> f64 {
        ((members.len() - 1) as f64).powf(setting_properties.alpha)
    }
    fn get_category_id(&self) -> &'static str {
        std::any::type_name::<T>()
            .rsplit("::")
            .next()
            .unwrap_or_default()
    }
}

impl<T: SettingCategory> SettingId<T> {
    pub fn new(_category: T, id: usize) -> SettingId<T> {
        SettingId {
            id,
            _phantom: std::marker::PhantomData::<T>,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItineraryEntry {
    pub setting: Box<dyn AnySettingId>,
    ratio: f64,
}

impl ItineraryEntry {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(setting: impl AnySettingId, ratio: f64) -> ItineraryEntry {
        let boxed_setting = Box::new(setting);
        ItineraryEntry {
            setting: boxed_setting,
            ratio,
        }
    }
}

pub fn append_itinerary_entry(
    itinerary: &mut Vec<ItineraryEntry>,
    context: &Context,
    setting: impl AnySettingId,
    nondefault_ratio: Option<f64>,
) -> Result<(), IxaError> {
    // Is this setting type registered? Our population loader is hard coded to always try to put
    // people in the core setting types, but sometimes we don't want all the core setting types
    // (we didn't specify them). So, first check that the setting in question exists.
    if context
        .get_data(SettingDataPlugin)
        .setting_properties
        .contains_key(&setting.get_type_id())
    {
        let ratio = match nondefault_ratio {
            Some(user_input) => user_input,
            None => get_itinerary_ratio(context, &setting)?,
        };
        itinerary.push(ItineraryEntry::new(setting, ratio));
    }
    Ok(())
}

// In the future, this method could take the person id as an argument for making individual-level
// itineraries.
fn get_itinerary_ratio(context: &Context, setting: &dyn AnySettingId) -> Result<f64, IxaError> {
    let itinerary_ratio = context
        .get_data(SettingDataPlugin)
        .default_itinerary_ratios
        .get(&setting.get_type_id());

    match itinerary_ratio {
        Some(ratio) => Ok(*ratio),
        None => Err(IxaError::from("Itinerary ratio not specified")),
    }
}

#[derive(Default)]
struct SettingDataContainer {
    setting_categories: HashSet<TypeId>,
    // For each setting type (e.g., Home) store the properties (e.g., alpha)
    setting_properties: HashMap<TypeId, SettingProperties>,
    // For each setting type there is a default itinerary ratio
    default_itinerary_ratios: HashMap<TypeId, f64>,
    // For each setting type, have a map of each setting id and a list of members
    all_members: HashMap<(TypeId, usize), IndexSet<EntityId<Person>>>,
    itineraries: HashMap<EntityId<Person>, Vec<ItineraryEntry>>,
}

impl SettingDataContainer {
    fn get_setting_members(
        &self,
        setting: &dyn AnySettingId,
    ) -> Option<&IndexSet<EntityId<Person>>> {
        self.all_members.get(&setting.get_tuple_id())
    }
    fn get_itinerary(&self, person_id: EntityId<Person>) -> Option<&Vec<ItineraryEntry>> {
        self.itineraries.get(&person_id)
    }
    fn with_itinerary<F>(&self, person_id: EntityId<Person>, mut callback: F)
    where
        F: FnMut(&dyn AnySettingId, &SettingProperties, &IndexSet<EntityId<Person>>, f64),
    {
        if let Some(itinerary) = self.get_itinerary(person_id) {
            for entry in itinerary {
                let setting = entry.setting.as_ref();
                let setting_props = self
                    .setting_properties
                    .get(&entry.setting.get_type_id())
                    .unwrap();
                let members = self.get_setting_members(setting).unwrap();
                callback(setting, setting_props, members, entry.ratio);
            }
        }
    }
    fn activate_itinerary(
        &mut self,
        person_id: EntityId<Person>,
        itinerary: &Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        let _span = open_span("activate itinerary");
        for itinerary_entry in itinerary {
            // TODO: If we are changing a person's itinerary, the person_id should be removed from vector
            // This isn't the same as the concept of being present or not.
            if !self
                .setting_categories
                .contains(&itinerary_entry.setting.get_type_id())
            {
                return Err(IxaError::from(
                    "Itinerary entry setting type not registered",
                ));
            }
            self.set_member(
                person_id,
                itinerary_entry.ratio,
                itinerary_entry.setting.get_tuple_id(),
            );
        }
        Ok(())
    }
    fn set_member(
        &mut self,
        person_id: EntityId<Person>,
        ratio: f64,
        setting_identifier: (TypeId, usize),
    ) {
        if ratio > 0.0 {
            self.add_member(person_id, setting_identifier);
        }
    }

    fn add_member(&mut self, person_id: EntityId<Person>, setting_identifier: (TypeId, usize)) {
        self.all_members
            .entry(setting_identifier)
            .or_default()
            .insert(person_id);
    }
}

#[macro_export]
macro_rules! define_setting_category {
    ($name:ident) => {
        #[derive(Default, Copy, Clone, Debug, Hash, Eq, PartialEq)]
        pub struct $name;

        impl $crate::settings::SettingCategory for $name {
            fn get_type_id(&self) -> std::any::TypeId {
                std::any::TypeId::of::<$name>()
            }
        }
    };
}

define_setting_category!(Home);
define_setting_category!(CensusTract);
define_setting_category!(School);
define_setting_category!(Workplace);

define_data_plugin!(
    SettingDataPlugin,
    SettingDataContainer,
    SettingDataContainer::default()
);

trait ContextSettingInternalExt: PluginContext + ContextRandomExt {
    fn get_setting_members_internal(
        &self,
        setting: &dyn AnySettingId,
    ) -> Option<&IndexSet<EntityId<Person>>> {
        self.get_data(SettingDataPlugin)
            .get_setting_members(setting)
    }

    fn sample_setting_members_internal(
        &self,
        setting: &dyn AnySettingId,
    ) -> Option<EntityId<Person>> {
        if let Some(members) = self
            .get_data(SettingDataPlugin)
            .get_setting_members(setting)
        {
            if members.is_empty() {
                return None;
            }
            let person = members[self.sample_range(SettingsRng, 0..members.len())];
            return Some(person);
        }
        None
    }

    fn get_itinerary_internal(&self, person_id: EntityId<Person>) -> Option<&Vec<ItineraryEntry>> {
        self.get_data(SettingDataPlugin).get_itinerary(person_id)
    }
}
impl ContextSettingInternalExt for Context {}

fn validate_itinerary(itinerary: &[ItineraryEntry]) -> Result<(), IxaError> {
    let mut setting_counts: HashMap<TypeId, HashSet<usize>> = HashMap::new();
    let _span = open_span("validate_modified_itinerary");
    for itinerary_entry in itinerary {
        let setting_id = itinerary_entry.setting.id();
        let setting_type = itinerary_entry.setting.get_type_id();
        if setting_counts
            .get(&setting_type)
            .is_some_and(|set| set.contains(&setting_id))
        {
            return Err(IxaError::from("Duplicated setting".to_string()));
        }
        setting_counts
            .entry(setting_type)
            .or_default()
            .insert(setting_id);

        if itinerary_entry.ratio < 0.0 {
            return Err(IxaError::from(
                "Setting ratio must be greater than or equal to 0".to_string(),
            ));
        }
    }
    Ok(())
}

#[allow(private_bounds)]
pub trait ContextSettingExt:
    PluginContext + ContextSettingInternalExt + ContextRandomExt + ContextEntitiesExt
{
    #[allow(dead_code)]
    fn get_setting_properties(
        &self,
        setting: &dyn SettingCategory,
    ) -> Result<SettingProperties, IxaError> {
        let data_container = self.get_data(SettingDataPlugin);

        match data_container
            .setting_properties
            .get(&setting.get_type_id())
        {
            None => Err(IxaError::from(
                "Attempting to get properties of unregistered setting type",
            )),
            Some(properties) => Ok(*properties),
        }
    }

    fn get_setting_itinerary_ratio(&self, setting: &dyn SettingCategory) -> Result<f64, IxaError> {
        let data_container = self.get_data(SettingDataPlugin);

        match data_container
            .default_itinerary_ratios
            .get(&setting.get_type_id())
        {
            None => Err(IxaError::from(
                "Attempting to get itinerary ratio of unregistered setting type",
            )),
            Some(ratio) => Ok(*ratio),
        }
    }

    fn register_setting_category(
        &mut self,
        setting: &dyn SettingCategory,
        setting_props: SettingProperties,
        default_itinerary_ratio: f64,
    ) -> Result<(), IxaError> {
        let container = self.get_data_mut(SettingDataPlugin);

        if !container.setting_categories.insert(setting.get_type_id()) {
            return Err(IxaError::from("Setting type is already registered"));
        }

        // Add properties
        container
            .setting_properties
            .insert(setting.get_type_id(), setting_props);

        container
            .default_itinerary_ratios
            .insert(setting.get_type_id(), default_itinerary_ratio);

        Ok(())
    }

    fn add_itinerary(
        &mut self,
        person_id: EntityId<Person>,
        itinerary: Vec<ItineraryEntry>,
    ) -> Result<(), IxaError> {
        let _span = open_span("add_itinerary");
        // Normalize itinerary ratios
        validate_itinerary(&itinerary)?;

        let total_ratio: f64 = itinerary.iter().map(|entry| entry.ratio).sum();
        // If we passed validation, we know setting entries aren't all zero, so we can divide by
        // total_ratio without worrying about dividing by zero.
        let mut itinerary = itinerary;
        for entry in &mut itinerary {
            entry.ratio /= total_ratio;
        }
        let container = self.get_data_mut(SettingDataPlugin);

        // Clean up settings that from previous itinerary, if there is one

        if container.itineraries.contains_key(&person_id) {
            return Err(IxaError::from("Person already has an itinerary."));
        }

        container.activate_itinerary(person_id, &itinerary)?;
        container.itineraries.insert(person_id, itinerary);

        Ok(())
    }

    #[allow(dead_code)]
    fn get_itinerary(&self, person_id: EntityId<Person>) -> Option<&Vec<ItineraryEntry>> {
        self.get_itinerary_internal(person_id)
    }

    #[allow(dead_code)]
    fn get_setting_members(
        &self,
        setting: &dyn AnySettingId,
    ) -> Option<&IndexSet<EntityId<Person>>> {
        self.get_setting_members_internal(setting)
    }

    /// Get the total current infectiousness multiplier for a person
    /// This is the sum of the infectiousness multipliers for each setting derived from the itinerary
    /// with members filtered as Active and in the Current itinerary
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of active members in the setting
    fn calculate_current_infectiousness_multiplier_for_person(
        &self,
        person_id: EntityId<Person>,
    ) -> f64 {
        let container = self.get_data(SettingDataPlugin);
        let mut collector = 0.0;
        container.with_itinerary(person_id, |setting, setting_props, members, ratio| {
            let multiplier: f64 = if members.is_empty() {
                0.0
            } else {
                setting.calculate_multiplier(members, *setting_props)
            };
            collector += ratio * multiplier;
        });
        collector
    }
    /// Get the maximum infectiousness multiplier for a person across all settings
    /// derived from both the default and modified itineraries of the person.
    /// These are generated without modification from the general formula of ratio * (N - 1) ^ alpha
    /// where N is the number of all active and inactive members in the setting
    fn calculate_max_infectiousness_multiplier_for_person(
        &self,
        person_id: EntityId<Person>,
    ) -> f64 {
        let container = self.get_data(SettingDataPlugin);
        let mut collector = 0.0;
        container.with_itinerary(person_id, |setting, setting_props, members, _ratio| {
            let multiplier: f64 = setting.calculate_multiplier(members, *setting_props);
            // We want to identify the max at the setting level, not itinerary level, so that we sample at the true maximum possible rate
            collector = f64::max(collector, multiplier);
        });
        collector
    }

    fn sample_from_setting_with_exclusion(
        &self,
        person_id: EntityId<Person>,
        setting: &dyn AnySettingId,
    ) -> Result<Option<EntityId<Person>>, IxaError> {
        let _span = open_span("get_contact");
        if let Some(members) = self.get_setting_members_internal(setting) {
            if members.get(&person_id).is_some() && members.len() == 1 {
                return Ok(None);
            }
            let mut contact_id;
            loop {
                contact_id = self.sample_setting_members_internal(setting);

                if contact_id != Some(person_id) {
                    break;
                }
            }
            return Ok(contact_id);
        }
        Err(IxaError::from("Group membership is None"))
    }

    fn sample_current_setting(&self, person_id: EntityId<Person>) -> Option<&dyn AnySettingId> {
        let _span = open_span("sample_setting");
        let container = self.get_data(SettingDataPlugin);
        let mut itinerary_multiplier = Vec::new();
        container.with_itinerary(person_id, |setting, setting_props, members, ratio| {
            let multiplier = if members.is_empty() {
                0.0
            } else {
                setting.calculate_multiplier(members, *setting_props)
            };
            itinerary_multiplier.push(ratio * multiplier);
        });

        let setting_index = self.sample_weighted(SettingsRng, &itinerary_multiplier);

        if let Some(itinerary) = self.get_itinerary_internal(person_id) {
            let itinerary_entry = &itinerary[setting_index];
            Some(itinerary_entry.setting.as_ref())
        } else {
            None
        }
    }

    fn sample_setting_members(&self, setting: &dyn AnySettingId) -> Option<EntityId<Person>> {
        self.sample_setting_members_internal(setting)
    }
}
impl ContextSettingExt for Context {}

pub fn init(context: &mut Context) {
    let Params {
        settings_properties,
        itinerary_ratios,
        ..
    } = context.get_params();

    let itinerary_ratios = itinerary_ratios.clone();
    for (setting_category, setting_properties) in settings_properties.clone() {
        match setting_category {
            CoreSettingsTypes::Home => {
                context
                    .register_setting_category(
                        &Home,
                        setting_properties,
                        itinerary_ratios
                            .get(&CoreSettingsTypes::Home)
                            .cloned()
                            .unwrap_or(0.0),
                    )
                    .unwrap();
            }
            CoreSettingsTypes::CensusTract => {
                context
                    .register_setting_category(
                        &CensusTract,
                        setting_properties,
                        itinerary_ratios
                            .get(&CoreSettingsTypes::CensusTract)
                            .cloned()
                            .unwrap_or(0.0),
                    )
                    .unwrap();
            }
            CoreSettingsTypes::School => {
                context
                    .register_setting_category(
                        &School,
                        setting_properties,
                        itinerary_ratios
                            .get(&CoreSettingsTypes::School)
                            .cloned()
                            .unwrap_or(0.0),
                    )
                    .unwrap();
            }
            CoreSettingsTypes::Workplace => {
                context
                    .register_setting_category(
                        &Workplace,
                        setting_properties,
                        itinerary_ratios
                            .get(&CoreSettingsTypes::Workplace)
                            .cloned()
                            .unwrap_or(0.0),
                    )
                    .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Age, parameters::GlobalParams, settings::ContextSettingExt};
    use ixa::{ContextEntitiesExt, ContextGlobalPropertiesExt, assert_almost_eq};

    define_setting_category!(Community);

    fn register_default_settings(context: &mut Context) {
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        context
            .register_setting_category(&Workplace, SettingProperties { alpha: 0.3 }, 1.0)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
            .unwrap();

        context
            .register_setting_category(&School, SettingProperties { alpha: 0.01 }, 1.0)
            .unwrap();
    }

    #[test]
    fn test_setting_category_creation() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 0.5)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.001 }, 0.25)
            .unwrap();
        let home_props = context.get_setting_properties(&Home).unwrap();
        let tract_props = context.get_setting_properties(&CensusTract).unwrap();

        let home_ratio = context.get_setting_itinerary_ratio(&Home).unwrap();
        let tract_ratio = context.get_setting_itinerary_ratio(&CensusTract).unwrap();

        assert_almost_eq!(0.1, home_props.alpha, 0.0);
        assert_eq!(0.5, home_ratio);
        assert_almost_eq!(0.001, tract_props.alpha, 0.0);
        assert_eq!(0.25, tract_ratio);
    }

    #[test]
    fn test_get_properties_after_registration() {
        let mut context = Context::new();
        let e = context.get_setting_properties(&Home).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Attempting to get properties of unregistered setting type"
                );
            }
            Some(ue) => panic!(
                "Expected an error setting plugin data is none. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        context.get_setting_properties(&Home).unwrap();
        let e = context.get_setting_properties(&CensusTract).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Attempting to get properties of unregistered setting type"
                );
            }
            Some(ue) => panic!(
                "Expected an error attempting to get properties of unregistered setting type. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }

        context.get_setting_itinerary_ratio(&Home).unwrap();
        let e = context.get_setting_itinerary_ratio(&CensusTract).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "Attempting to get itinerary ratio of unregistered setting type"
                );
            }
            Some(ue) => panic!(
                "Expected an error attempting to get itinerary ratio of unregistered setting type. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_duplicate_setting_category_registration() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        let e = context
            .register_setting_category(&Home, SettingProperties { alpha: 0.001 }, 1.0)
            .err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Setting type is already registered");
            }
            Some(ue) => panic!(
                "Expected an error that there are duplicate settings types. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_duplicated_itinerary() {
        let mut context = Context::new();
        register_default_settings(&mut context);

        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
        ];
        let e = context.add_itinerary(person, itinerary).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Duplicated setting");
            }
            Some(ue) => panic!(
                "Expected an error that there are duplicate settings. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_feasible_itinerary_ratio() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 2), -0.5)];

        let e = context.add_itinerary(person, itinerary).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Setting ratio must be greater than or equal to 0");
            }
            Some(ue) => panic!(
                "Expected an error setting ratios should be greater than or equal to 0. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_feasible_itinerary_setting() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();

        // Community is a defined setting but not registered
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Community, 2), 0.5)];

        let e = context.add_itinerary(person, itinerary).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Itinerary entry setting type not registered");
            }
            Some(ue) => panic!(
                "Expected an error setting . Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_change_activity_members() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let active_person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let inactive_person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let active_itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 1), 1.0)];
        let inactive_itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 1), 0.0)];
        context
            .add_itinerary(active_person, active_itinerary.clone())
            .unwrap();
        context
            .add_itinerary(inactive_person, inactive_itinerary.clone())
            .unwrap();

        let home = SettingId::new(Home, 1);

        let members = context.get_setting_members_internal(&home).unwrap();

        assert_eq!(members.len(), 1);
    }

    #[test]
    fn test_add_itinerary() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
        ];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(&SettingId::new(Home, 2))
            .unwrap();
        assert_eq!(members.len(), 1);

        let person2 = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary2 = vec![ItineraryEntry::new(SettingId::new(Home, 2), 1.0)];
        context.add_itinerary(person2, itinerary2).unwrap();

        let members2 = context
            .get_setting_members(&SettingId::new(Home, 2))
            .unwrap();
        assert_eq!(members2.len(), 2);

        let itinerary3 = vec![ItineraryEntry::new(SettingId::new(Home, 3), 0.5)];

        let e = context.add_itinerary(person, itinerary3).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Person already has an itinerary.");
            }
            Some(ue) => panic!(
                "Expected an error setting . Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_get_itinerary() {
        let mut context = Context::new();
        register_default_settings(&mut context);
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let default_itinerary = vec![
            ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
            ItineraryEntry::new(SettingId::new(Home, 2), 0.5),
        ];
        context.add_itinerary(person, default_itinerary).unwrap();

        let default = context.get_itinerary(person).unwrap();

        for entry in default {
            assert_almost_eq!(entry.ratio, 0.5, 0.0);
        }
    }

    #[test]
    fn test_setting_registration() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
            .unwrap();
        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::new(Home, s), 0.5),
                    ItineraryEntry::new(SettingId::new(CensusTract, s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
            let members = context
                .get_setting_members(&SettingId::new(Home, s))
                .unwrap();
            let tract_members = context
                .get_setting_members(&SettingId::new(CensusTract, s))
                .unwrap();

            assert_eq!(members.len(), 5);
            assert_eq!(tract_members.len(), 5);
        }
    }

    #[test]
    fn test_setting_registration_activity() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
            .unwrap();
        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::new(Home, s), 1.0),
                    ItineraryEntry::new(SettingId::new(CensusTract, s), 0.0),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
            let all_home_members = context
                .get_setting_members_internal(&SettingId::new(Home, s))
                .unwrap();
            let all_tract_members =
                context.get_setting_members_internal(&SettingId::new(CensusTract, s));

            assert_eq!(all_home_members.len(), 5);
            assert_eq!(all_tract_members, None);
        }
    }

    #[test]
    fn test_setting_multiplier() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        for s in 0..5 {
            // Create 5 people
            for _ in 0..5 {
                let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
                let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, s), 0.5)];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }

        let home_id = 0;
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, home_id), 0.5)];
        context.add_itinerary(person, itinerary).unwrap();
        let members = context
            .get_setting_members(&SettingId::new(Home, home_id))
            .unwrap();

        let setting_type = &SettingId::new(Home, home_id);

        let inf_multiplier =
            setting_type.calculate_multiplier(members, SettingProperties { alpha: 0.1 });

        // This is assuming we know what the function for Home is (N - 1) ^ alpha
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);
    }

    #[test]
    fn test_total_infectiousness_multiplier() {
        // Go through all the settings and compute infectiousness multiplier
        let mut context = Context::new();
        register_default_settings(&mut context);

        for s in 0..5 {
            for _ in 0..5 {
                let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
                let itinerary = vec![
                    ItineraryEntry::new(SettingId::new(Home, s), 0.5),
                    ItineraryEntry::new(SettingId::new(CensusTract, s), 0.5),
                ];
                context.add_itinerary(person, itinerary).unwrap();
            }
        }
        // Create a new person and register to home 0
        let itinerary = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context.add_itinerary(person, itinerary).unwrap();

        // If only registered at home, total infectiousness multiplier should be (6 - 1) ^ (alpha)
        let inf_multiplier = context.calculate_max_infectiousness_multiplier_for_person(person);
        assert_almost_eq!(inf_multiplier, f64::from(6 - 1).powf(0.1), 0.0);

        // If person's itinerary is changed for two settings,
        // CensusTract 0 should have 6 members, Home 0 should have 7 members
        // the total infectiousness should be the sum of infs * proportion
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary_complete = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
        ];
        context.add_itinerary(person, itinerary_complete).unwrap();
        let members_home = context
            .get_setting_members(&SettingId::new(Home, 0))
            .unwrap();
        let members_tract = context
            .get_setting_members(&SettingId::new(CensusTract, 0))
            .unwrap();
        assert_eq!(members_home.len(), 7);
        assert_eq!(members_tract.len(), 6);

        let inf_multiplier_two_settings =
            context.calculate_max_infectiousness_multiplier_for_person(person);

        let alpha_h = context.get_setting_properties(&Home).unwrap().alpha;
        let alpha_ct = context.get_setting_properties(&CensusTract).unwrap().alpha;

        assert_almost_eq!(
            inf_multiplier_two_settings,
            f64::max(
                f64::from(7 - 1).powf(alpha_h),
                f64::from(6 - 1).powf(alpha_ct)
            ),
            0.0
        );
    }

    #[test]
    fn test_sample_setting() {
        let mut context = Context::new();
        context.init_random(42);
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
            .unwrap();

        let person_a = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let person_b = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();

        // When person a is used to select a setting for contact, it should return Home. While they are
        // also a member of CensusTract, since they are the only member the multiplier used to weight the
        // selection is 0.0 from calculate_multiplier. Thus the probability CensusTract is selected is 0.0.
        let setting_id = context.sample_current_setting(person_a).unwrap();
        assert_eq!(setting_id.get_type_id(), TypeId::of::<Home>());
        assert_eq!(setting_id.id(), 0);

        let setting_id = context.sample_current_setting(person_b).unwrap();
        assert_eq!(setting_id.get_type_id(), TypeId::of::<Home>());
        assert_eq!(setting_id.id(), 0);

        let person_c = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();
        let setting_id = context.sample_current_setting(person_c).unwrap();
        assert_eq!(setting_id.get_type_id(), TypeId::of::<CensusTract>());
        assert_eq!(setting_id.id(), 0);
    }

    #[test]
    fn test_get_contact_from_setting() {
        // Register two people to a setting and make sure that the person chosen is the other one
        // Attempt to draw a contact from a setting with only the person trying to get a contact
        let mut context = Context::new();
        context.init_random(42);
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 1.0)
            .unwrap();

        let person_a = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let person_b = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary_a = vec![
            ItineraryEntry::new(SettingId::new(Home, 0), 0.5),
            ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5),
        ];
        let itinerary_b = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
        context.add_itinerary(person_a, itinerary_a).unwrap();
        context.add_itinerary(person_b, itinerary_b).unwrap();
        let setting_id = context.sample_current_setting(person_a).unwrap();
        let members = context.get_setting_members(setting_id).unwrap();
        assert!(members.contains(&person_a));

        assert_eq!(
            Some(person_b),
            context
                .sample_from_setting_with_exclusion(person_a, setting_id)
                .unwrap()
        );

        assert!(
            context
                .sample_from_setting_with_exclusion(person_a, &SettingId::new(CensusTract, 0))
                .unwrap()
                .is_none()
        );

        let person_c = context.add_entity::<Person, _>((Age(30),)).unwrap();
        let itinerary_c = vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 0.5)];
        context.add_itinerary(person_c, itinerary_c).unwrap();

        let e =
            context.sample_from_setting_with_exclusion(person_a, &SettingId::new(CensusTract, 10));
        match e {
            Err(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Group membership is None");
            }
            Err(ue) => panic!(
                "Expected an error attempting contact outside group membership. Instead got: {:?}",
                ue.to_string()
            ),
            Ok(_) => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_default_itinerary_ratio_specification() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 1.0)
            .unwrap();
        let e = get_itinerary_ratio(&context, &SettingId::new(CensusTract, 0)).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Itinerary ratio not specified");
            }
            Some(ue) => panic!(
                "Expected an error that itinerary specification is not specified. Instead got: {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_append_itinerary_entry() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 0.5)
            .unwrap();
        context
            .register_setting_category(&School, SettingProperties { alpha: 0.2 }, 0.25)
            .unwrap();
        let mut itinerary = vec![];

        // Test appending a valid entry
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 1), None).unwrap();
        assert_eq!(itinerary.len(), 1);
        assert_eq!(itinerary[0].setting.get_type_id(), TypeId::of::<Home>());
        assert_eq!(itinerary[0].setting.id(), 1);
        assert_almost_eq!(itinerary[0].ratio, 0.5, 0.0);

        // Test appending an entry with a different setting type
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(School, 42), None).unwrap();
        assert_eq!(itinerary.len(), 2);
        assert_eq!(itinerary[1].setting.get_type_id(), TypeId::of::<School>());
        assert_eq!(itinerary[1].setting.id(), 42);
        assert_almost_eq!(itinerary[1].ratio, 0.25, 0.0);

        // Test appending an entry with a non-default ratio
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 2), Some(1.0))
            .unwrap();
        assert_eq!(itinerary.len(), 3);
        assert_eq!(itinerary[2].setting.get_type_id(), TypeId::of::<Home>());
        assert_eq!(itinerary[2].setting.id(), 2);
        assert_almost_eq!(itinerary[2].ratio, 1.0, 0.0);
    }

    #[test]
    fn test_get_itinerary_ratio() {
        let mut context = Context::new();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 0.5)
            .unwrap();

        // Test with a valid setting type
        let ratio = get_itinerary_ratio(&context, &SettingId::new(Home, 0)).unwrap();
        assert_almost_eq!(ratio, 0.5, 0.0);
    }

    #[test]
    fn test_only_include_registered_settings_in_itineraries() {
        let mut context = Context::new();
        let parameters = Params {
            settings_properties: HashMap::from_iter(
                [(CoreSettingsTypes::Home, SettingProperties { alpha: 0.5 })]
                    .into_iter()
                    .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter(
                [(CoreSettingsTypes::Home, 0.5)]
                    .into_iter()
                    .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };

        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        init(&mut context);
        let mut iitinerary = vec![];
        append_itinerary_entry(
            &mut iitinerary,
            &context,
            SettingId::new(Workplace, 1),
            None,
        )
        .unwrap();

        assert_eq!(iitinerary.len(), 0);

        append_itinerary_entry(&mut iitinerary, &context, SettingId::new(Home, 1), None).unwrap();
        assert_eq!(iitinerary.len(), 1);
        assert_eq!(iitinerary[0].setting.get_type_id(), TypeId::of::<Home>());
    }

    #[test]
    fn test_itinerary_normalized_one() {
        let mut context = Context::new();
        let person = context.add_entity::<Person, _>((Age(30),)).unwrap();
        context
            .register_setting_category(&Home, SettingProperties { alpha: 0.1 }, 5.0)
            .unwrap();
        context
            .register_setting_category(&CensusTract, SettingProperties { alpha: 0.01 }, 2.5)
            .unwrap();
        context
            .register_setting_category(&School, SettingProperties { alpha: 0.2 }, 2.5)
            .unwrap();

        // Test creating an itinerary with all settings
        let mut itinerary = vec![];
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(Home, 1), None).unwrap();
        append_itinerary_entry(
            &mut itinerary,
            &context,
            SettingId::new(CensusTract, 1),
            None,
        )
        .unwrap();
        append_itinerary_entry(&mut itinerary, &context, SettingId::new(School, 1), None).unwrap();

        context.add_itinerary(person, itinerary).unwrap();
        let itinerary = context.get_itinerary(person).unwrap();

        let total_ratio: Vec<f64> = itinerary.iter().map(|entry| entry.ratio).collect();
        assert_eq!(total_ratio, vec![0.5, 0.25, 0.25]);
    }
}
