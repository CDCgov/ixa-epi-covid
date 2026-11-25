use std::{
    any::Any,
    ops::{Index, IndexMut},
};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    geography::{GEOGRAPHY_COUNT, Geography, GeographyDiscriminants},
    isolation_guidance::{
        EligibleForIsolation, IsolationGuidanceModifiers, TimeBasedIsolationGuidance,
    },
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, create_itinerary_transition_matrix,
    },
    pop_reader::{FIPSCode, states::USState},
    settings::{ContextSettingExt, Itinerary, Person, PersonId, SETTING_COUNT, SettingCategory},
    transmission_modifier_manager::{ContextTransmissionModifierExt, ScalarTransmissionModifier},
    triggers::{ContextCustomTriggerExt, CustomTrigger},
};
use ixa::{HashMap, HashMapExt, IxaEvent, impl_derived_property, prelude::*};
use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter};

define_rng!(InterventionRng);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolState(pub Option<USState>);
impl_derived_property!(SchoolState, Person, [Itinerary], [], |itinerary| {
    SchoolState(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| Some(code.0.state().unwrap()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolCounty(pub Option<FIPSCode>);
impl_derived_property!(SchoolCounty, Person, [Itinerary], [], |itinerary| {
    SchoolCounty(
        itinerary.setting_ids[SettingCategory::School]
            .and_then(|code| code.0.state_county_code().ok()),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkState(pub Option<USState>);
impl_derived_property!(WorkState, Person, [Itinerary], [], |itinerary| {
    WorkState(
        itinerary.setting_ids[SettingCategory::Work]
            .map(|code| Some(code.0.state().unwrap()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkCounty(pub Option<FIPSCode>);
impl_derived_property!(WorkCounty, Person, [Itinerary], [], |itinerary| {
    WorkCounty(
        itinerary.setting_ids[SettingCategory::Work]
            .and_then(|code| code.0.state_county_code().ok()),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct HomeState(pub Option<USState>);
impl_derived_property!(HomeState, Person, [Itinerary], [], |itinerary| {
    HomeState(
        itinerary.setting_ids[SettingCategory::Home]
            .map(|code| Some(code.0.state().unwrap()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct HomeCounty(pub Option<FIPSCode>);
impl_derived_property!(HomeCounty, Person, [Itinerary], [], |itinerary| {
    HomeCounty(
        itinerary.setting_ids[SettingCategory::Home]
            .and_then(|code| code.0.state_county_code().ok()),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct AcceptsIntervention(pub [[Option<Geography>; MODIFIER_COUNT]; GEOGRAPHY_COUNT]);
impl_property!(
    AcceptsIntervention,
    Person,
    default_const = AcceptsIntervention([[None; MODIFIER_COUNT]; GEOGRAPHY_COUNT])
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsSchoolClosureState(pub Option<Geography>);
impl_derived_property!(
    AcceptsSchoolClosureState,
    Person,
    [AcceptsIntervention],
    [],
    |accepts| {
        AcceptsSchoolClosureState(accepts.0[GeographyDiscriminants::State][Modifier::SchoolClosure])
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsSchoolClosureCounty(pub Option<Geography>);
impl_derived_property!(
    AcceptsSchoolClosureCounty,
    Person,
    [AcceptsIntervention],
    [],
    |accepts| {
        AcceptsSchoolClosureCounty(
            accepts.0[GeographyDiscriminants::County][Modifier::SchoolClosure],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsWorkMobilityState(pub Option<Geography>);
impl_derived_property!(
    AcceptsWorkMobilityState,
    Person,
    [AcceptsIntervention, WorkState],
    [],
    |accepts, work_state| {
        AcceptsWorkMobilityState(
            accepts.0[GeographyDiscriminants::State][Modifier::WorkplaceMobilityReduction],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsWorkMobilityCounty(pub Option<Geography>);
impl_derived_property!(
    AcceptsWorkMobilityCounty,
    Person,
    [AcceptsIntervention, WorkCounty],
    [],
    |accepts, work_county| {
        AcceptsWorkMobilityCounty(
            accepts.0[GeographyDiscriminants::County][Modifier::WorkplaceMobilityReduction],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsCommunityMobilityState(pub Option<Geography>);
impl_derived_property!(
    AcceptsCommunityMobilityState,
    Person,
    [AcceptsIntervention, HomeState],
    [],
    |accepts, home_state| {
        AcceptsCommunityMobilityState(
            accepts.0[GeographyDiscriminants::State][Modifier::CommunityMobilityReduction],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsCommunityMobilityCounty(pub Option<Geography>);
impl_derived_property!(
    AcceptsCommunityMobilityCounty,
    Person,
    [AcceptsIntervention, HomeCounty],
    [],
    |accepts, home_county| {
        AcceptsCommunityMobilityCounty(
            accepts.0[GeographyDiscriminants::County][Modifier::CommunityMobilityReduction],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsMaskMandateState(pub Option<Geography>);
impl_derived_property!(
    AcceptsMaskMandateState,
    Person,
    [AcceptsIntervention, HomeState],
    [],
    |accepts, home_state| {
        AcceptsMaskMandateState(accepts.0[GeographyDiscriminants::State][Modifier::MaskMandate])
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsMaskMandateCounty(pub Option<Geography>);
impl_derived_property!(
    AcceptsMaskMandateCounty,
    Person,
    [AcceptsIntervention, HomeCounty],
    [],
    |accepts, home_county| {
        AcceptsMaskMandateCounty(accepts.0[GeographyDiscriminants::County][Modifier::MaskMandate])
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsIsolationGuidanceState(pub Option<Geography>);
impl_derived_property!(
    AcceptsIsolationGuidanceState,
    Person,
    [AcceptsIntervention, HomeState],
    [],
    |accepts, home_state| {
        AcceptsIsolationGuidanceState(
            accepts.0[GeographyDiscriminants::State][Modifier::IsolationGuidance],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsIsolationGuidanceCounty(pub Option<Geography>);
impl_derived_property!(
    AcceptsIsolationGuidanceCounty,
    Person,
    [AcceptsIntervention, HomeCounty],
    [],
    |accepts, home_county| {
        AcceptsIsolationGuidanceCounty(
            accepts.0[GeographyDiscriminants::County][Modifier::IsolationGuidance],
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsAndIsolatesState(pub Option<Geography>);
impl_derived_property!(
    AcceptsAndIsolatesState,
    Person,
    [AcceptsIntervention, EligibleForIsolation],
    [],
    |accepts, isolation_status| {
        AcceptsAndIsolatesState(
            if accepts.0[GeographyDiscriminants::State][Modifier::IsolationGuidance].is_some()
                && isolation_status.0
            {
                accepts.0[GeographyDiscriminants::State][Modifier::IsolationGuidance]
            } else {
                None
            },
        )
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsAndIsolatesCounty(pub Option<Geography>);
impl_derived_property!(
    AcceptsAndIsolatesCounty,
    Person,
    [AcceptsIntervention, EligibleForIsolation],
    [],
    |accepts, isolation_status| {
        AcceptsAndIsolatesCounty(
            if accepts.0[GeographyDiscriminants::County][Modifier::IsolationGuidance].is_some()
                && isolation_status.0
            {
                accepts.0[GeographyDiscriminants::County][Modifier::IsolationGuidance]
            } else {
                None
            },
        )
    }
);

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize, Eq, Hash, EnumCount, EnumIter)]
#[repr(u8)]
pub enum Modifier {
    SchoolClosure = 0,
    CommunityMobilityReduction,
    WorkplaceMobilityReduction,
    MaskMandate,
    IsolationGuidance,
}

impl<T> Index<Modifier> for [T; MODIFIER_COUNT] {
    type Output = T;
    fn index(&self, index: Modifier) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<Modifier> for [T; MODIFIER_COUNT] {
    fn index_mut(&mut self, index: Modifier) -> &mut Self::Output {
        &mut self[index as usize]
    }
}

pub const MODIFIER_COUNT: usize = Modifier::COUNT;

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum InterventionModifierSpecification {
    Itinerary {
        home: Option<[f64; SETTING_COUNT]>,
        school: Option<[f64; SETTING_COUNT]>,
        work: Option<[f64; SETTING_COUNT]>,
        community: Option<[f64; SETTING_COUNT]>,
    },
    Transmission {
        infectiousness_multiplier: f64,
        susceptibility_multiplier: f64,
        settings: Vec<SettingCategory>,
    },
    IsolationGuidance {
        home: Option<[f64; SETTING_COUNT]>,
        school: Option<[f64; SETTING_COUNT]>,
        work: Option<[f64; SETTING_COUNT]>,
        community: Option<[f64; SETTING_COUNT]>,
        infectiousness_multiplier: f64,
        susceptibility_multiplier: f64,
        settings: Vec<SettingCategory>,
        delay_symptoms_to_isolation: f64,
        minimum_isolation_duration: f64,
        post_symptoms_isolation_duration: f64,
    },
}

impl InterventionModifierSpecification {
    fn get_transmission_modifier(&self) -> Option<ScalarTransmissionModifier> {
        match self {
            InterventionModifierSpecification::Transmission {
                infectiousness_multiplier,
                susceptibility_multiplier,
                settings,
            } => Some(ScalarTransmissionModifier {
                infectiousness_multiplier: *infectiousness_multiplier,
                susceptibility_multiplier: *susceptibility_multiplier,
                settings: settings.clone(),
                acceptance_function: None,
            }),
            _ => None,
        }
    }
    fn get_itinerary_modifier(&self) -> Option<[[f64; SETTING_COUNT]; SETTING_COUNT]> {
        match self {
            InterventionModifierSpecification::Itinerary {
                home,
                school,
                work,
                community,
            } => {
                let mut modifiers = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                if let Some(home) = home {
                    modifiers[SettingCategory::Home as usize] = *home;
                }
                if let Some(school) = school {
                    modifiers[SettingCategory::School as usize] = *school;
                }
                if let Some(work) = work {
                    modifiers[SettingCategory::Work as usize] = *work;
                }
                if let Some(community) = community {
                    modifiers[SettingCategory::Community as usize] = *community;
                }
                Some(modifiers)
            }
            _ => None,
        }
    }

    pub fn get_time_based_isolation_guidance_specification(
        &self,
    ) -> Option<TimeBasedIsolationGuidance> {
        match self {
            InterventionModifierSpecification::IsolationGuidance {
                delay_symptoms_to_isolation,
                minimum_isolation_duration,
                post_symptoms_isolation_duration,
                ..
            } => Some(TimeBasedIsolationGuidance {
                delay_symptoms_to_isolation: *delay_symptoms_to_isolation,
                minimum_isolation_duration: *minimum_isolation_duration,
                post_symptoms_isolation_duration: *post_symptoms_isolation_duration,
            }),
            _ => None,
        }
    }
    pub fn get_isolation_guidance_modifiers(
        &self,
    ) -> Option<(
        [[f64; SETTING_COUNT]; SETTING_COUNT],
        ScalarTransmissionModifier,
    )> {
        match self {
            InterventionModifierSpecification::IsolationGuidance {
                home,
                school,
                work,
                community,
                infectiousness_multiplier,
                susceptibility_multiplier,
                settings,
                ..
            } => {
                let mut modifiers = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                if let Some(home) = home {
                    modifiers[SettingCategory::Home as usize] = *home;
                }
                if let Some(school) = school {
                    modifiers[SettingCategory::School as usize] = *school;
                }
                if let Some(work) = work {
                    modifiers[SettingCategory::Work as usize] = *work;
                }
                if let Some(community) = community {
                    modifiers[SettingCategory::Community as usize] = *community;
                }
                Some((
                    modifiers,
                    ScalarTransmissionModifier {
                        infectiousness_multiplier: *infectiousness_multiplier,
                        susceptibility_multiplier: *susceptibility_multiplier,
                        settings: settings.clone(),
                        acceptance_function: None,
                    },
                ))
            }
            _ => None,
        }
    }
    pub fn validate(&self) -> Result<(), ModelError> {
        match self {
            InterventionModifierSpecification::Transmission {
                infectiousness_multiplier,
                susceptibility_multiplier,
                settings,
            } => {
                if !(0.0..=1.0).contains(infectiousness_multiplier) {
                    return Err(ModelError::ModelError(
                        "Infectiousness multiplier must be between 0 and 1".to_string(),
                    ));
                }
                if !(0.0..=1.0).contains(susceptibility_multiplier) {
                    return Err(ModelError::ModelError(
                        "Susceptibility multiplier must be between 0 and 1".to_string(),
                    ));
                }
                if settings.is_empty() {
                    return Err(ModelError::ModelError(
                        "Settings must not be empty".to_string(),
                    ));
                }
                Ok(())
            }
            InterventionModifierSpecification::Itinerary {
                home,
                school,
                work,
                community,
            } => {
                let categories = [
                    (SettingCategory::Home, home),
                    (SettingCategory::School, school),
                    (SettingCategory::Work, work),
                    (SettingCategory::Community, community),
                ];
                for (category, modifier) in categories.iter() {
                    if let Some(values) = modifier {
                        for &value in values.iter() {
                            if !(0.0..=1.0).contains(&value) {
                                return Err(ModelError::ModelError(format!(
                                    "{:?} modifier values must be between 0 and 1",
                                    category
                                )));
                            }
                        }
                    }
                }
                Ok(())
            }
            InterventionModifierSpecification::IsolationGuidance {
                home,
                school,
                work,
                community,
                infectiousness_multiplier,
                susceptibility_multiplier,
                settings,
                delay_symptoms_to_isolation,
                minimum_isolation_duration,
                post_symptoms_isolation_duration,
                ..
            } => {
                let categories = [
                    (SettingCategory::Home, home),
                    (SettingCategory::School, school),
                    (SettingCategory::Work, work),
                    (SettingCategory::Community, community),
                ];
                for (category, modifier) in categories.iter() {
                    if let Some(values) = modifier {
                        for &value in values.iter() {
                            if !(0.0..=1.0).contains(&value) {
                                return Err(ModelError::ModelError(format!(
                                    "{:?} modifier values must be between 0 and 1",
                                    category
                                )));
                            }
                        }
                    }
                }
                if !(0.0..=1.0).contains(infectiousness_multiplier) {
                    return Err(ModelError::ModelError(
                        "Infectiousness multiplier must be between 0 and 1".to_string(),
                    ));
                }
                if !(0.0..=1.0).contains(susceptibility_multiplier) {
                    return Err(ModelError::ModelError(
                        "Susceptibility multiplier must be between 0 and 1".to_string(),
                    ));
                }
                if settings.is_empty() {
                    return Err(ModelError::ModelError(
                        "Settings must not be empty".to_string(),
                    ));
                }
                if *delay_symptoms_to_isolation < 0.0 {
                    return Err(ModelError::ModelError(
                        "Delay symptoms to isolation must be greater than or equal to 0"
                            .to_string(),
                    ));
                }
                if *minimum_isolation_duration < 0.0 {
                    return Err(ModelError::ModelError(
                        "Minimum isolation duration must be greater than or equal to 0".to_string(),
                    ));
                }
                if *post_symptoms_isolation_duration < 0.0 {
                    return Err(ModelError::ModelError(
                        "Post symptoms isolation duration must be greater than or equal to 0"
                            .to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

pub trait ModifierSpecification {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Intervention {
    pub modifier: Modifier,
    pub geography: Geography,
    pub acceptance_probability: f64,
    pub trigger: CustomTrigger,
}

impl Intervention {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !(0.0..=1.0).contains(&self.acceptance_probability) {
            return Err(ModelError::ModelError(
                "Intervention acceptance probability must be between 0 and 1".to_string(),
            ));
        }
        self.trigger.validate()?;
        Ok(())
    }

    pub fn validate_overlap(&self, other: &Intervention) -> Result<(), ModelError> {
        if self.modifier == other.modifier && self.geography == other.geography {
            self.trigger.check_overlap(other.trigger)?;
        }
        Ok(())
    }
}
#[derive(IxaEvent)]
struct InterventionEvent {
    active: bool,
    intervention: Intervention,
}

#[derive(Default)]
pub struct InterventionData {
    is_active: HashMap<(Modifier, Geography), bool>,
}

impl InterventionData {
    pub fn new() -> Self {
        Self {
            is_active: HashMap::<(Modifier, Geography), bool>::new(),
        }
    }

    pub fn activate_intervention(&mut self, intervention: Intervention) {
        self.is_active
            .insert((intervention.modifier, intervention.geography), true);
    }

    pub fn deactivate_intervention(&mut self, intervention: Intervention) {
        self.is_active
            .insert((intervention.modifier, intervention.geography), false);
    }

    pub fn is_intervention_active(&self, modifier: Modifier, geography: Geography) -> bool {
        *self.is_active.get(&(modifier, geography)).unwrap_or(&false)
    }
}

define_data_plugin!(
    InterventionDataPlugin,
    InterventionData,
    InterventionData::default()
);

pub trait InterventionContextExt:
    PluginContext
    + ContextEntitiesExt
    + ContextParametersExt
    + ContextSettingExt
    + ContextCustomTriggerExt
    + ContextTransmissionModifierExt
{
    fn setup_intervention_triggers(&mut self, intervention: Intervention) {
        self.set_custom_trigger(
            intervention.trigger,
            InterventionEvent {
                active: true,
                intervention,
            },
            InterventionEvent {
                active: false,
                intervention,
            },
        );
    }

    fn setup_intervention_trigger_event_subscription(&mut self) {
        self.subscribe_to_event(move |context, event: InterventionEvent| {
            if event.active {
                context.set_accepts_intervention(event.intervention);
                context
                    .register_intervention_itinerary_modifier(event.intervention)
                    .unwrap();
                let data = context.get_data_mut(InterventionDataPlugin);
                data.activate_intervention(event.intervention);
            } else {
                context
                    .remove_intervention_itinerary_modifier(event.intervention)
                    .unwrap();
                let data = context.get_data_mut(InterventionDataPlugin);
                data.deactivate_intervention(event.intervention);
            }
        });
    }

    fn set_accepts_intervention(&mut self, intervention: Intervention) {
        // Not including the geography in the registration person property could lead to
        // not handling overriding itinerary modifiers correctly.
        let people: Vec<PersonId> = match (intervention.geography, intervention.modifier) {
            (Geography::State(code), Modifier::SchoolClosure) => self
                .query_result_iterator(with!(Person, SchoolState(Some(code))))
                .collect(),
            (Geography::County(code), Modifier::SchoolClosure) => self
                .query_result_iterator(with!(Person, SchoolCounty(Some(code))))
                .collect(),
            (Geography::State(code), Modifier::WorkplaceMobilityReduction) => self
                .query_result_iterator(with!(Person, WorkState(Some(code))))
                .collect(),
            (Geography::County(code), Modifier::WorkplaceMobilityReduction) => self
                .query_result_iterator(with!(Person, WorkCounty(Some(code))))
                .collect(),
            (Geography::State(code), Modifier::CommunityMobilityReduction) => self
                .query_result_iterator(with!(Person, HomeState(Some(code))))
                .collect(),
            (Geography::County(code), Modifier::CommunityMobilityReduction) => self
                .query_result_iterator(with!(Person, HomeCounty(Some(code))))
                .collect(),
            (Geography::State(code), Modifier::MaskMandate) => self
                .query_result_iterator(with!(Person, HomeState(Some(code))))
                .collect(),
            (Geography::County(code), Modifier::MaskMandate) => self
                .query_result_iterator(with!(Person, HomeCounty(Some(code))))
                .collect(),
            (Geography::State(code), Modifier::IsolationGuidance) => self
                .query_result_iterator(with!(Person, HomeState(Some(code))))
                .collect(),
            (Geography::County(code), Modifier::IsolationGuidance) => self
                .query_result_iterator(with!(Person, HomeCounty(Some(code))))
                .collect(),
        };
        for person in people {
            let accepts = self.sample_bool(InterventionRng, intervention.acceptance_probability);
            let mut current_accepts = self.get_property::<Person, AcceptsIntervention>(person);
            if accepts {
                current_accepts.0[intervention.geography][intervention.modifier] =
                    Some(intervention.geography);
            }
            self.set_property(person, current_accepts);
        }
    }

    fn register_intervention_itinerary_modifier(
        &mut self,
        intervention: Intervention,
    ) -> Result<(), ModelError> {
        let modifier = self.get_modifier_specification(intervention);
        match (intervention.modifier, intervention.geography) {
            (Modifier::SchoolClosure, Geography::State(state_code)) => {
                self.setup_itinerary_modifier(
                    AcceptsSchoolClosureState(Some(Geography::State(state_code))),
                    *modifier
                        .into_any()
                        .downcast::<ItineraryTransitionMatrix>()
                        .expect("expected ItineraryTransitionMatrix"),
                    false,
                );
            }
            (Modifier::SchoolClosure, Geography::County(fips_code)) => {
                self.setup_itinerary_modifier(
                    AcceptsSchoolClosureCounty(Some(Geography::County(fips_code))),
                    *modifier
                        .into_any()
                        .downcast::<ItineraryTransitionMatrix>()
                        .expect("expected ItineraryTransitionMatrix"),
                    false,
                );
            }
            (Modifier::WorkplaceMobilityReduction, Geography::State(state_code)) => {
                self.setup_itinerary_modifier(
                    AcceptsWorkMobilityState(Some(Geography::State(state_code))),
                    *modifier
                        .into_any()
                        .downcast::<ItineraryTransitionMatrix>()
                        .expect("expected ItineraryTransitionMatrix"),
                    true,
                );
            }
            (Modifier::WorkplaceMobilityReduction, Geography::County(fips_code)) => {
                self.setup_itinerary_modifier(
                    AcceptsWorkMobilityCounty(Some(Geography::County(fips_code))),
                    *modifier
                        .into_any()
                        .downcast::<ItineraryTransitionMatrix>()
                        .expect("expected ItineraryTransitionMatrix"),
                    true,
                );
            }
            (Modifier::CommunityMobilityReduction, Geography::State(state_code)) => {
                self.setup_itinerary_modifier(
                    AcceptsCommunityMobilityState(Some(Geography::State(state_code))),
                    *modifier
                        .into_any()
                        .downcast::<ItineraryTransitionMatrix>()
                        .expect("expected ItineraryTransitionMatrix"),
                    true,
                );
            }
            (Modifier::CommunityMobilityReduction, Geography::County(fips_code)) => {
                self.setup_itinerary_modifier(
                    AcceptsCommunityMobilityCounty(Some(Geography::County(fips_code))),
                    *modifier
                        .into_any()
                        .downcast::<ItineraryTransitionMatrix>()
                        .expect("expected ItineraryTransitionMatrix"),
                    true,
                );
            }
            (Modifier::MaskMandate, Geography::State(state_code)) => {
                self.register_transmission_modifier([(
                    AcceptsMaskMandateState(Some(Geography::State(state_code))),
                    *modifier
                        .into_any()
                        .downcast::<ScalarTransmissionModifier>()
                        .expect("expected ScalarTransmissionModifier"),
                )])
                .unwrap();
            }
            (Modifier::MaskMandate, Geography::County(fips_code)) => {
                self.register_transmission_modifier([(
                    AcceptsMaskMandateCounty(Some(Geography::County(fips_code))),
                    *modifier
                        .into_any()
                        .downcast::<ScalarTransmissionModifier>()
                        .expect("expected ScalarTransmissionModifier"),
                )])
                .unwrap();
            }
            (Modifier::IsolationGuidance, Geography::State(state_code)) => {
                let modifier = *modifier
                    .into_any()
                    .downcast::<IsolationGuidanceModifiers>()
                    .expect("expected IsolationGuidanceModifiers");
                let itinerary_modifier = modifier
                    .itinerary_modifier
                    .into_any()
                    .downcast::<ItineraryTransitionMatrix>()
                    .expect("expected ItineraryTransitionMatrix");
                let transmission_modifier = modifier
                    .transmission_modifier
                    .into_any()
                    .downcast::<ScalarTransmissionModifier>()
                    .expect("expected ScalarTransmissionModifier");
                self.setup_itinerary_modifier(
                    AcceptsAndIsolatesState(Some(Geography::State(state_code))),
                    *itinerary_modifier,
                    false,
                );
                self.register_transmission_modifier([(
                    AcceptsAndIsolatesState(Some(Geography::State(state_code))),
                    *transmission_modifier,
                )])
                .unwrap();
            }
            (Modifier::IsolationGuidance, Geography::County(fips_code)) => {
                let modifier = *modifier
                    .into_any()
                    .downcast::<IsolationGuidanceModifiers>()
                    .expect("expected IsolationGuidanceModifiers");
                let itinerary_modifier = modifier
                    .itinerary_modifier
                    .into_any()
                    .downcast::<ItineraryTransitionMatrix>()
                    .expect("expected ItineraryTransitionMatrix");
                let transmission_modifier = modifier
                    .transmission_modifier
                    .into_any()
                    .downcast::<ScalarTransmissionModifier>()
                    .expect("expected ScalarTransmissionModifier");
                self.setup_itinerary_modifier(
                    AcceptsAndIsolatesCounty(Some(Geography::County(fips_code))),
                    *itinerary_modifier,
                    false,
                );
                self.register_transmission_modifier([(
                    AcceptsAndIsolatesCounty(Some(Geography::County(fips_code))),
                    *transmission_modifier,
                )])
                .unwrap();
            }
        }
        Ok(())
    }
    fn remove_intervention_itinerary_modifier(
        &mut self,
        intervention: Intervention,
    ) -> Result<(), ModelError> {
        match (intervention.modifier, intervention.geography) {
            (Modifier::SchoolClosure, Geography::State(state_code)) => {
                self.remove_itinerary_modifier(AcceptsSchoolClosureState(Some(Geography::State(
                    state_code,
                ))));
            }
            (Modifier::SchoolClosure, Geography::County(fips_code)) => {
                self.remove_itinerary_modifier(AcceptsSchoolClosureCounty(Some(
                    Geography::County(fips_code),
                )));
            }
            (Modifier::WorkplaceMobilityReduction, Geography::State(state_code)) => {
                self.remove_itinerary_modifier(AcceptsWorkMobilityState(Some(Geography::State(
                    state_code,
                ))));
            }
            (Modifier::WorkplaceMobilityReduction, Geography::County(fips_code)) => {
                self.remove_itinerary_modifier(AcceptsWorkMobilityCounty(Some(Geography::County(
                    fips_code,
                ))));
            }
            (Modifier::CommunityMobilityReduction, Geography::State(state_code)) => {
                self.remove_itinerary_modifier(AcceptsCommunityMobilityState(Some(
                    Geography::State(state_code),
                )));
            }
            (Modifier::CommunityMobilityReduction, Geography::County(fips_code)) => {
                self.remove_itinerary_modifier(AcceptsCommunityMobilityCounty(Some(
                    Geography::County(fips_code),
                )));
            }
            (Modifier::MaskMandate, Geography::State(state_code)) => {
                self.remove_transmission_modifier(AcceptsMaskMandateState(Some(Geography::State(
                    state_code,
                ))));
            }
            (Modifier::MaskMandate, Geography::County(fips_code)) => {
                self.remove_transmission_modifier(AcceptsMaskMandateCounty(Some(
                    Geography::County(fips_code),
                )));
            }
            (Modifier::IsolationGuidance, Geography::State(state_code)) => {
                self.remove_itinerary_modifier(AcceptsAndIsolatesState(Some(Geography::State(
                    state_code,
                ))));
                self.remove_transmission_modifier(AcceptsAndIsolatesState(Some(Geography::State(
                    state_code,
                ))));
            }
            (Modifier::IsolationGuidance, Geography::County(fips_code)) => {
                self.remove_itinerary_modifier(AcceptsAndIsolatesCounty(Some(Geography::County(
                    fips_code,
                ))));
                self.remove_transmission_modifier(AcceptsAndIsolatesCounty(Some(
                    Geography::County(fips_code),
                )));
            }
        }
        Ok(())
    }

    fn is_intervention_active(&self, modifier: Modifier, geography: Geography) -> bool {
        let data = self.get_data(InterventionDataPlugin);
        data.is_intervention_active(modifier, geography)
    }

    fn get_modifier_specification(
        &self,
        intervention: Intervention,
    ) -> Box<dyn ModifierSpecification> {
        let default_modifiers = self.get_params().default_modifiers.clone();
        let modifier_spec = default_modifiers
            .get(&intervention.modifier)
            .expect("Modifier parameters not found");
        match intervention.modifier {
            Modifier::SchoolClosure => {
                let matrix = modifier_spec
                    .get_itinerary_modifier()
                    .expect("Itinerary modifiers incorrectly specified");
                Box::new(create_itinerary_transition_matrix(
                    None,
                    Some(matrix),
                    Some(create_intervention_acceptance_function(intervention)),
                ))
            }
            Modifier::WorkplaceMobilityReduction => {
                let matrix = modifier_spec
                    .get_itinerary_modifier()
                    .expect("Itinerary modifiers incorrectly specified");
                Box::new(create_itinerary_transition_matrix(
                    None,
                    Some(matrix),
                    Some(create_intervention_acceptance_function(intervention)),
                ))
            }
            Modifier::CommunityMobilityReduction => {
                let matrix = modifier_spec
                    .get_itinerary_modifier()
                    .expect("Itinerary modifiers incorrectly specified");
                Box::new(create_itinerary_transition_matrix(
                    Some(matrix),
                    None,
                    Some(create_intervention_acceptance_function(intervention)),
                ))
            }
            Modifier::MaskMandate => {
                let transmission_multipliers = modifier_spec
                    .get_transmission_modifier()
                    .expect("Transmission modifiers incorrectly specified");
                Box::new(ScalarTransmissionModifier {
                    infectiousness_multiplier: transmission_multipliers.infectiousness_multiplier,
                    susceptibility_multiplier: transmission_multipliers.susceptibility_multiplier,
                    settings: transmission_multipliers.settings.clone(),
                    acceptance_function: Some(create_intervention_acceptance_function(
                        intervention,
                    )),
                })
            }
            Modifier::IsolationGuidance => {
                let modifiers = modifier_spec
                    .get_isolation_guidance_modifiers()
                    .expect("Isolation guidance modifiers incorrectly specified");
                let itinerary_modifier = create_itinerary_transition_matrix(
                    Some(modifiers.0),
                    None,
                    Some(create_intervention_acceptance_function(intervention)),
                );
                let transmission_modifier = ScalarTransmissionModifier {
                    infectiousness_multiplier: modifiers.1.infectiousness_multiplier,
                    susceptibility_multiplier: modifiers.1.susceptibility_multiplier,
                    settings: modifiers.1.settings.clone(),
                    acceptance_function: Some(create_intervention_acceptance_function(
                        intervention,
                    )),
                };
                let isolation_guidance = IsolationGuidanceModifiers {
                    itinerary_modifier: Box::new(itinerary_modifier),
                    transmission_modifier: Box::new(transmission_modifier),
                };
                Box::new(isolation_guidance)
            }
        }
    }
}
impl InterventionContextExt for Context {}

fn create_intervention_acceptance_function(intervention: Intervention) -> AcceptanceFunction {
    Box::new(move |context, _person| match intervention.geography {
        Geography::State(_) => {
            context.is_intervention_active(intervention.modifier, intervention.geography)
        }
        Geography::County(code) => {
            !context.is_intervention_active(
                intervention.modifier,
                Geography::State(code.state().unwrap()),
            ) && context.is_intervention_active(intervention.modifier, intervention.geography)
        }
    })
}

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    context.index_property::<Person, SchoolState>();
    context.index_property::<Person, SchoolCounty>();
    context.index_property::<Person, WorkState>();
    context.index_property::<Person, WorkCounty>();
    context.index_property::<Person, HomeState>();
    context.index_property::<Person, HomeCounty>();
    let Params { interventions, .. } = context.get_params().clone();
    for intervention in interventions {
        context.setup_intervention_triggers(intervention);
    }
    context.setup_intervention_trigger_event_subscription();
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        Age,
        infectiousness_manager::InfectionData,
        isolation_guidance::time_based_isolation_guidance,
        itinerary_manager::ContextItineraryModifierExt,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::parser::parse_fips_school_id,
        settings::SettingCode,
        symptom_status_manager::SymptomData,
    };
    use ixa::{ExecutionPhase, HashMap, assert_almost_eq};
    use serde_json::json;
    use std::{cell::RefCell, panic, rc::Rc};

    use super::*;
    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }
    fn make_work_id(work_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(work_id).unwrap().1)
    }
    fn make_home_id(home_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(home_id).unwrap().1)
    }
    fn make_community_id(community_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(community_id).unwrap().1)
    }

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
            default_modifiers: HashMap::from_iter([
                (
                    Modifier::SchoolClosure,
                    InterventionModifierSpecification::Itinerary {
                        home: None,
                        school: Some([1.0, 0.0, 0.0, 0.0]),
                        work: None,
                        community: None,
                    },
                ),
                (
                    Modifier::WorkplaceMobilityReduction,
                    InterventionModifierSpecification::Itinerary {
                        home: None,
                        school: None,
                        work: Some([1.0, 0.0, 0.0, 0.0]),
                        community: None,
                    },
                ),
                (
                    Modifier::CommunityMobilityReduction,
                    InterventionModifierSpecification::Itinerary {
                        home: None,
                        school: None,
                        work: None,
                        community: Some([1.0, 0.0, 0.0, 0.0]),
                    },
                ),
                (
                    Modifier::MaskMandate,
                    InterventionModifierSpecification::Transmission {
                        infectiousness_multiplier: 0.9,
                        susceptibility_multiplier: 0.9,
                        settings: vec![
                            SettingCategory::Work,
                            SettingCategory::School,
                            SettingCategory::Community,
                        ],
                    },
                ),
                (
                    Modifier::IsolationGuidance,
                    InterventionModifierSpecification::IsolationGuidance {
                        home: None,
                        school: Some([1.0, 0.0, 0.0, 0.0]),
                        work: Some([1.0, 0.0, 0.0, 0.0]),
                        community: Some([1.0, 0.0, 0.0, 0.0]),
                        infectiousness_multiplier: 0.9,
                        susceptibility_multiplier: 0.9,
                        settings: vec![SettingCategory::Home],
                        post_symptoms_isolation_duration: 3.0,
                        minimum_isolation_duration: 7.0,
                        delay_symptoms_to_isolation: 1.0,
                    },
                ),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_setup_intervention_triggers() {
        let mut context = setup();
        let intervention_starts = Rc::new(RefCell::new(0));
        let intervention_starts_clone: Rc<RefCell<usize>> = Rc::clone(&intervention_starts);
        let intervention_ends = Rc::new(RefCell::new(0));
        let intervention_ends_clone: Rc<RefCell<usize>> = Rc::clone(&intervention_ends);
        let g1 = Geography::State(USState::decode(1).unwrap());
        let school_closure = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };
        let work_place_mobility_reduction = Intervention {
            modifier: Modifier::WorkplaceMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 3.0,
                duration: 1.0,
            },
        };
        let community_mobility_reduction = Intervention {
            modifier: Modifier::CommunityMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 5.0,
                duration: 1.0,
            },
        };
        context.setup_intervention_triggers(school_closure);
        context.setup_intervention_triggers(work_place_mobility_reduction);
        context.setup_intervention_triggers(community_mobility_reduction);
        context.setup_intervention_trigger_event_subscription();
        context.subscribe_to_event(move |cxt, event: InterventionEvent| {
            if event.active && event.intervention.geography == g1 {
                if event.intervention.modifier == Modifier::SchoolClosure {
                    assert_eq!(cxt.get_current_time(), 1.0);
                } else if event.intervention.modifier == Modifier::WorkplaceMobilityReduction {
                    assert_eq!(cxt.get_current_time(), 3.0);
                } else if event.intervention.modifier == Modifier::CommunityMobilityReduction {
                    assert_eq!(cxt.get_current_time(), 5.0);
                }
                *intervention_starts_clone.borrow_mut() += 1;
            } else {
                if event.intervention.modifier == Modifier::SchoolClosure {
                    assert_eq!(cxt.get_current_time(), 2.0);
                } else if event.intervention.modifier == Modifier::WorkplaceMobilityReduction {
                    assert_eq!(cxt.get_current_time(), 4.0);
                } else if event.intervention.modifier == Modifier::CommunityMobilityReduction {
                    assert_eq!(cxt.get_current_time(), 6.0);
                }
                *intervention_ends_clone.borrow_mut() += 1;
            }
        });
        context.add_plan_with_phase(7.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_intervention_starts = *intervention_starts.borrow();
        let observed_intervention_ends = *intervention_ends.borrow();
        // interventions start on day 1, 3, 5
        // interventions end on day 2, 4, 6
        assert_eq!(observed_intervention_starts, 3);
        assert_eq!(observed_intervention_ends, 3);
    }

    #[test]
    fn test_setup_intervention_itinerary_modification() {
        let mut context = setup();
        let school_code = make_school_id(b"16037960200002");
        let work_code = make_work_id(b"16037960200003");
        let home_code = make_home_id(b"16037960200004");
        let community_code = make_community_id(b"16037960200004");
        let g1 = Geography::State(school_code.0.state().unwrap());
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let school_closure = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };
        let work_place_mobility_reduction = Intervention {
            modifier: Modifier::WorkplaceMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 3.0,
                duration: 1.0,
            },
        };
        let community_mobility_reduction = Intervention {
            modifier: Modifier::CommunityMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 5.0,
                duration: 1.0,
            },
        };
        context.setup_intervention_triggers(school_closure);
        context.setup_intervention_triggers(work_place_mobility_reduction);
        context.setup_intervention_triggers(community_mobility_reduction);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    Some(work_code),
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.2, 0.3, 0.2],
            },
        );

        context.add_plan(0.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.2, 0.3, 0.2]);
        });
        context.add_plan(1.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.6, 0.2, 0.0, 0.2]);
        });
        context.add_plan(2.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.2, 0.3, 0.2]);
        });
        context.add_plan(3.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.5, 0.0, 0.3, 0.2]);
        });
        context.add_plan(4.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.2, 0.3, 0.2]);
        });
        context.add_plan(5.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.5, 0.2, 0.3, 0.0]);
        });
        context.execute();
    }

    #[test]
    fn test_intervention_setup_transmission_modification() {
        let mut context = setup();
        let school_code = make_school_id(b"16037960200002");
        let work_code = make_work_id(b"16037960200003");
        let home_code = make_home_id(b"16037960200004");
        let community_code = make_community_id(b"16037960200004");
        let g1 = Geography::State(school_code.0.state().unwrap());
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let mask_mandate = Intervention {
            modifier: Modifier::MaskMandate,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };

        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    Some(work_code),
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.2, 0.3, 0.2],
            },
        );

        context.set_property::<Person, InfectionData>(
            p1,
            InfectionData::Infectious {
                infection_time: 1.2,
                infected_by: None,
                infection_setting_id: None,
            },
        );

        context.setup_intervention_triggers(mask_mandate);
        context.setup_intervention_trigger_event_subscription();

        context.add_plan(0.0, move |context| {
            let transmission = context.get_relative_total_transmission(p1, SettingCategory::School);
            assert_eq!(transmission, 1.0);
        });
        context.add_plan(1.5, move |context| {
            let transmission = context.get_relative_total_transmission(p1, SettingCategory::School);
            assert_eq!(transmission, 0.9);
        });
        context.add_plan(2.5, move |context| {
            let transmission = context.get_relative_total_transmission(p1, SettingCategory::School);
            assert_eq!(transmission, 1.0);
        });
        context.execute();
    }

    #[test]
    fn test_intervention_acceptance_probability() {
        let mut context = setup();
        let acceptance = Rc::new(RefCell::new(0));
        let acceptance_clone: Rc<RefCell<usize>> = Rc::clone(&acceptance);
        let school_code = make_school_id(b"16037960200002");
        let g1 = Geography::State(school_code.0.state().unwrap());
        let pop_size = 10000;
        for _ in 0..pop_size {
            let person = context.add_entity(with!(Person, Age(10))).unwrap();
            context.set_property(
                person,
                Itinerary {
                    setting_ids: [None, None, Some(school_code), None],
                    itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
                },
            );
        }
        let intervention = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 0.5,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };
        context.setup_intervention_triggers(intervention);
        context.setup_intervention_trigger_event_subscription();

        context.add_plan(1.5, move |context| {
            let people = context.get_entity_iterator::<Person>();
            for person in people {
                let accepts = context.get_property::<Person, AcceptsIntervention>(person);
                if accepts.0[g1][Modifier::SchoolClosure] == Some(g1) {
                    *acceptance_clone.borrow_mut() += 1;
                }
            }
        });
        context.execute();

        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_acceptance = *acceptance.borrow();
        assert_almost_eq!(observed_acceptance as f64 / pop_size as f64, 0.5, 0.025);
    }

    #[test]
    fn modifier_specification_rejects_unknown_fields() {
        let values = [1.0; SETTING_COUNT];
        let input = json!({"Itinerary":{
            "home": values,
            "unexpected": values
        }});

        let result = serde_json::from_value::<InterventionModifierSpecification>(input);

        assert!(result.is_err());

        let error = result.unwrap_err().to_string();
        assert!(error.contains("unknown field"));
        assert!(error.contains("unexpected"));
    }

    #[test]
    fn modifier_specification_accepts_known_fields() {
        let values = [1.0; SETTING_COUNT];
        let input = json!({"Itinerary":{
            "home": values,
            "school": null,
            "work": values,
            "community": null
        }});

        let result = serde_json::from_value::<InterventionModifierSpecification>(input);

        assert!(result.is_ok());
    }

    #[test]
    fn test_itinerary_modifier_specification_validation() {
        // Valid modifier specification - all values sum to 1.0 and are in [0, 1]
        let valid_modifier = InterventionModifierSpecification::Itinerary {
            home: Some([0.3, 0.2, 0.3, 0.2]),
            school: Some([1.0, 0.0, 0.0, 0.0]),
            work: None,
            community: None,
        };
        assert!(valid_modifier.validate().is_ok());

        // Invalid - values outside [0, 1]
        let invalid_range = InterventionModifierSpecification::Itinerary {
            home: Some([1.5, -0.5, 0.0, 0.0]),
            school: None,
            work: None,
            community: None,
        };
        assert!(invalid_range.validate().is_err());

        // Valid - all categories with valid values
        let all_categories = InterventionModifierSpecification::Itinerary {
            home: Some([0.5, 0.25, 0.25, 0.0]),
            school: Some([0.0, 1.0, 0.0, 0.0]),
            work: Some([0.2, 0.3, 0.4, 0.1]),
            community: Some([0.1, 0.1, 0.1, 0.7]),
        };
        assert!(all_categories.validate().is_ok());

        // Valid - all None
        let all_none = InterventionModifierSpecification::Itinerary {
            home: None,
            school: None,
            work: None,
            community: None,
        };
        assert!(all_none.validate().is_ok());
    }

    #[test]
    fn test_transmission_modifier_specification_validation() {
        // Valid transmission modifier specification
        let valid_modifier = InterventionModifierSpecification::Transmission {
            infectiousness_multiplier: 0.9,
            susceptibility_multiplier: 0.9,
            settings: vec![
                SettingCategory::Home,
                SettingCategory::Work,
                SettingCategory::School,
                SettingCategory::Community,
            ],
        };
        assert!(valid_modifier.validate().is_ok());

        // Invalid - modifier outside [0, 1]
        let invalid_modifier = InterventionModifierSpecification::Transmission {
            infectiousness_multiplier: 1.5,
            susceptibility_multiplier: 1.5,
            settings: vec![
                SettingCategory::Home,
                SettingCategory::Work,
                SettingCategory::School,
                SettingCategory::Community,
            ],
        };
        assert!(invalid_modifier.validate().is_err());

        let no_settings = InterventionModifierSpecification::Transmission {
            infectiousness_multiplier: 0.9,
            susceptibility_multiplier: 0.9,
            settings: vec![],
        };
        assert!(no_settings.validate().is_err());
    }

    #[test]
    fn test_intervention_validation() {
        // Valid intervention with all fields
        let valid_intervention = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(USState::decode(1).unwrap()),
            acceptance_probability: 0.95,
            trigger: CustomTrigger::Time {
                start_time: 10.0,
                duration: 5.0,
            },
        };
        assert!(valid_intervention.validate().is_ok());

        // Invalid - acceptance probability > 1.0
        let invalid_probability_high = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(USState::decode(1).unwrap()),
            acceptance_probability: 1.5,
            trigger: CustomTrigger::Time {
                start_time: 10.0,
                duration: 5.0,
            },
        };
        assert!(invalid_probability_high.validate().is_err());

        // Invalid - acceptance probability < 0.0
        let invalid_probability_low = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(USState::decode(1).unwrap()),
            acceptance_probability: -0.1,
            trigger: CustomTrigger::Time {
                start_time: 10.0,
                duration: 5.0,
            },
        };
        assert!(invalid_probability_low.validate().is_err());

        // Invalid - negative activation time
        let invalid_activation = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(USState::decode(1).unwrap()),
            acceptance_probability: 0.5,
            trigger: CustomTrigger::Time {
                start_time: -1.0,
                duration: 5.0,
            },
        };
        assert!(invalid_activation.validate().is_err());

        // Invalid - non-positive duration
        let invalid_duration = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(USState::decode(1).unwrap()),
            acceptance_probability: 0.5,
            trigger: CustomTrigger::Time {
                start_time: 10.0,
                duration: 0.0,
            },
        };
        assert!(invalid_duration.validate().is_err());
    }

    #[test]
    fn test_different_modifiers_in_different_geographies() {
        let school_code1 = make_school_id(b"16037960200002");
        let school_code2 = make_school_id(b"16038960200002");
        let home_code = make_home_id(b"16037960200002");
        let community_code = home_code.extract_community();
        let g1 = Geography::County(school_code1.0.state_county_code().unwrap());
        let g2 = Geography::County(community_code.0.state_county_code().unwrap());
        let intervention1 = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };
        let intervention2 = Intervention {
            modifier: Modifier::CommunityMobilityReduction,
            geography: g2,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };

        let mut context = setup();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_intervention_triggers(intervention1);
        context.setup_intervention_triggers(intervention2);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code1),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.set_property(
            p2,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code2),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.add_plan(0.0, move |context| {
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
        });
        context.add_plan(1.5, move |context| {
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(itinerary2, [0.5, 0.0, 0.5, 0.0]);
        });
        context.add_plan(3.0, move |context| {
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
        });
        context.execute();
    }

    #[test]
    fn test_mask_mandate_intervention() {
        let school_code = make_school_id(b"16037960200002");
        let home_code = make_home_id(b"16037960200002");
        let home_code2 = make_home_id(b"1677760200003");
        let community_code = home_code.extract_community();
        let community_code2 = home_code2.extract_community();
        let g1 = Geography::County(home_code.0.state_county_code().unwrap());
        let intervention1 = Intervention {
            modifier: Modifier::MaskMandate,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 1.0,
            },
        };

        let mut context = setup();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_intervention_triggers(intervention1);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.set_property(
            p2,
            Itinerary {
                setting_ids: [
                    Some(home_code2),
                    None,
                    Some(school_code),
                    Some(community_code2),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.add_plan(0.0, move |context| {
            let transmission1 =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2 =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });
        context.add_plan(1.5, move |context| {
            let transmission1 =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2 =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1, 0.9);
            assert_eq!(transmission2, 1.0);
        });
        context.add_plan(3.0, move |context| {
            let transmission1 =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2 =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });
        context.execute();
    }

    #[test]
    fn test_isolation_guidance_modifiers() {
        let school_code = make_school_id(b"16037960200002");
        let home_code = make_home_id(b"16037960200002");
        let community_code = home_code.extract_community();
        let g1 = Geography::State(school_code.0.state().unwrap());
        let intervention1 = Intervention {
            modifier: Modifier::IsolationGuidance,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 10.0,
            },
        };

        let mut context = setup();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_intervention_triggers(intervention1);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.set_property(
            p2,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        time_based_isolation_guidance::init(&mut context).unwrap();
        context.add_plan(0.0, move |context| {
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
        });
        context.add_plan(0.25, move |context| {
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Infectious {
                    infection_time: 0.25,
                    infected_by: None,
                    infection_setting_id: None,
                },
            );
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 0.25 });
        });
        context.add_plan(1.5, move |context| {
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 0.9);
            assert_eq!(transmission2, 1.0);
        });
        context.add_plan(3.0, move |context| {
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Recovered {
                    infection_time: 0.25,
                    recovery_time: 3.0,
                },
            );
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 0.25,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 3.0,
                },
            );
        });
        context.add_plan(4.0, move |context| {
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });
        context.add_plan(8.0, move |context| {
            assert!(!context.get_property::<Person, EligibleForIsolation>(p1).0);
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });
        context.execute();
    }

    #[test]
    fn test_isolation_guidance_and_mask_mandate() {
        let school_code = make_school_id(b"16037960200003");
        let home_code = make_home_id(b"16037960200002");
        let community_code = home_code.extract_community();
        let g1 = Geography::State(home_code.0.state().unwrap());
        let intervention1 = Intervention {
            modifier: Modifier::IsolationGuidance,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 10.0,
            },
        };
        let intervention2 = Intervention {
            modifier: Modifier::MaskMandate,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 10.0,
            },
        };

        let mut context = setup();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_intervention_triggers(intervention1);
        context.setup_intervention_triggers(intervention2);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.set_property(
            p2,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        time_based_isolation_guidance::init(&mut context).unwrap();
        context.add_plan(0.0, move |context| {
            // no interventions have been activated
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2 =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });
        context.add_plan(1.5, move |context| {
            // interventions have been activated but only mask mandate will have an affect
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2 =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1, 0.9);
            assert_eq!(transmission2, 0.9);
        });

        context.add_plan(2.0, move |context| {
            // person p1 becomes infectious and symptomatic so the isolation will affect them
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Infectious {
                    infection_time: 2.0,
                    infected_by: None,
                    infection_setting_id: None,
                },
            );
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 2.0 });
        });

        context.add_plan(4.0, move |context| {
            // p1 will now be impacted by the isolation intervention since they are infectious and symptomatic
            // p2 will still only be impacted by the mask mandate
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1_home =
                context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission1_community =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2_home =
                context.get_relative_total_transmission(p2, SettingCategory::Home);
            let transmission2_community =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1_home, 0.9);
            assert_eq!(transmission1_community, 0.9);
            assert_eq!(transmission2_home, 1.0);
            assert_eq!(transmission2_community, 0.9);
        });

        context.add_plan(5.0, move |context| {
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Recovered {
                    infection_time: 2.0,
                    recovery_time: 5.0,
                },
            );
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 2.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 5.0,
                },
            );
        });

        context.add_plan(9.5, move |context| {
            // p1 will have exited isolation guidance and the mask mandate will not
            // be observable because they are recovered
            // p2 will still only be impacted by the mask mandate
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1_home =
                context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission1_community =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2_home =
                context.get_relative_total_transmission(p2, SettingCategory::Home);
            let transmission2_community =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1_home, 1.0);
            assert_eq!(transmission1_community, 1.0);
            assert_eq!(transmission2_home, 1.0);
            assert_eq!(transmission2_community, 0.9);
        });

        context.add_plan(12.0, move |context| {
            // no interventions apply
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 =
                context.get_relative_total_transmission(p1, SettingCategory::Community);
            let transmission2 =
                context.get_relative_total_transmission(p2, SettingCategory::Community);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });

        context.execute();
    }

    #[test]
    fn test_isolation_guidance_and_community_mobility_reduction() {
        let school_code = make_school_id(b"16037960200003");
        let home_code = make_home_id(b"16037960200002");
        let community_code = home_code.extract_community();
        let g1 = Geography::State(home_code.0.state().unwrap());
        let intervention1 = Intervention {
            modifier: Modifier::IsolationGuidance,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 10.0,
            },
        };
        let intervention2 = Intervention {
            modifier: Modifier::CommunityMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            trigger: CustomTrigger::Time {
                start_time: 1.0,
                duration: 10.0,
            },
        };

        let mut context = setup();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_intervention_triggers(intervention1);
        context.setup_intervention_triggers(intervention2);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.set_property(
            p2,
            Itinerary {
                setting_ids: [
                    Some(home_code),
                    None,
                    Some(school_code),
                    Some(community_code),
                ],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        time_based_isolation_guidance::init(&mut context).unwrap();
        context.add_plan(0.0, move |context| {
            // no interventions have been activated
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });
        context.add_plan(1.5, move |context| {
            // interventions have been activated but only community mobility reduction will have an affect
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.5, 0.0, 0.5, 0.0]);
            assert_eq!(itinerary2, [0.5, 0.0, 0.5, 0.0]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });

        context.add_plan(2.0, move |context| {
            // person p1 becomes infectious and symptomatic so the isolation will affect them
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Infectious {
                    infection_time: 2.0,
                    infected_by: None,
                    infection_setting_id: None,
                },
            );
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 2.0 });
        });

        context.add_plan(4.0, move |context| {
            // p1 will now be impacted by the isolation + community mobility reduction intervention
            // since they are infectious and symptomatic
            // p2 will still only be impacted by the community mobility reduction
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(itinerary2, [0.5, 0.0, 0.5, 0.0]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 0.9);
            assert_eq!(transmission2, 1.0);
        });

        context.add_plan(5.0, move |context| {
            context.set_property::<Person, InfectionData>(
                p1,
                InfectionData::Recovered {
                    infection_time: 2.0,
                    recovery_time: 5.0,
                },
            );
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 2.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 5.0,
                },
            );
        });

        context.add_plan(9.5, move |context| {
            // p1 will have exited isolation guidance and the community mobility reduction will
            // still apply
            // p2 will still only be impacted by the community mobility reduction
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.5, 0.0, 0.5, 0.0]);
            assert_eq!(itinerary2, [0.5, 0.0, 0.5, 0.0]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });

        context.add_plan(12.0, move |context| {
            // no interventions apply
            let itinerary1 = context.get_itinerary(p1);
            let itinerary2 = context.get_itinerary(p2);
            assert_eq!(itinerary1, [0.3, 0.0, 0.5, 0.2]);
            assert_eq!(itinerary2, [0.3, 0.0, 0.5, 0.2]);
            let transmission1 = context.get_relative_total_transmission(p1, SettingCategory::Home);
            let transmission2 = context.get_relative_total_transmission(p2, SettingCategory::Home);
            assert_eq!(transmission1, 1.0);
            assert_eq!(transmission2, 1.0);
        });

        context.execute();
    }
}
