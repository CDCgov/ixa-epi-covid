use std::ops::{Index, IndexMut};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    geography::{GEOGRAPHY_COUNT, Geography},
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, create_itinerary_transition_matrix,
    },
    pop_reader::{FIPSCode, StateCode},
    settings::{
        ContextSettingExt, Itinerary, Person, PersonId, SETTING_COUNT, SettingCategory,
        SettingMembershipChange,
    },
};
use ixa::{
    ExecutionPhase, HashMap, HashMapExt, IxaEvent, impl_derived_property,
    prelude::*,
    triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter};

define_rng!(InterventionRng);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolState(pub Option<StateCode>);
impl_derived_property!(SchoolState, Person, [Itinerary], [], |itinerary| {
    SchoolState(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| Some(code.0.state_code()))
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
pub struct WorkState(pub Option<StateCode>);
impl_derived_property!(WorkState, Person, [Itinerary], [], |itinerary| {
    WorkState(
        itinerary.setting_ids[SettingCategory::Work]
            .map(|code| Some(code.0.state_code()))
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
pub struct HomeState(pub Option<StateCode>);
impl_derived_property!(HomeState, Person, [Itinerary], [], |itinerary| {
    HomeState(
        itinerary.setting_ids[SettingCategory::Home]
            .map(|code| Some(code.0.state_code()))
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsIntervention(pub [[bool; MODIFIER_COUNT]; GEOGRAPHY_COUNT]);
impl_property!(
    AcceptsIntervention,
    Person,
    default_const = AcceptsIntervention([[false; MODIFIER_COUNT]; GEOGRAPHY_COUNT])
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsSchoolClosureState(pub bool);
impl_derived_property!(
    AcceptsSchoolClosureState,
    Person,
    [AcceptsIntervention, SchoolState],
    [],
    |accepts, school_state| {
        AcceptsSchoolClosureState(school_state.0.is_some_and(|state_code| {
            accepts.0[Geography::State(state_code)][Modifier::SchoolClosure]
        }))
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsSchoolClosureCounty(pub bool);
impl_derived_property!(
    AcceptsSchoolClosureCounty,
    Person,
    [AcceptsIntervention, SchoolCounty],
    [],
    |accepts, school_county| {
        AcceptsSchoolClosureCounty(school_county.0.is_some_and(|fips_code| {
            accepts.0[Geography::County(fips_code)][Modifier::SchoolClosure]
        }))
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsWorkMobilityState(pub bool);
impl_derived_property!(
    AcceptsWorkMobilityState,
    Person,
    [AcceptsIntervention, WorkState],
    [],
    |accepts, work_state| {
        AcceptsWorkMobilityState(work_state.0.is_some_and(|state_code| {
            accepts.0[Geography::State(state_code)][Modifier::WorkplaceMobilityReduction]
        }))
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsWorkMobilityCounty(pub bool);
impl_derived_property!(
    AcceptsWorkMobilityCounty,
    Person,
    [AcceptsIntervention, WorkCounty],
    [],
    |accepts, work_county| {
        AcceptsWorkMobilityCounty(work_county.0.is_some_and(|fips_code| {
            accepts.0[Geography::County(fips_code)][Modifier::WorkplaceMobilityReduction]
        }))
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsCommunityMobilityState(pub bool);
impl_derived_property!(
    AcceptsCommunityMobilityState,
    Person,
    [AcceptsIntervention, HomeState],
    [],
    |accepts, home_state| {
        AcceptsCommunityMobilityState(home_state.0.is_some_and(|state_code| {
            accepts.0[Geography::State(state_code)][Modifier::CommunityMobilityReduction]
        }))
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct AcceptsCommunityMobilityCounty(pub bool);
impl_derived_property!(
    AcceptsCommunityMobilityCounty,
    Person,
    [AcceptsIntervention, HomeCounty],
    [],
    |accepts, home_county| {
        AcceptsCommunityMobilityCounty(home_county.0.is_some_and(|fips_code| {
            accepts.0[Geography::County(fips_code)][Modifier::CommunityMobilityReduction]
        }))
    }
);

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize, Eq, Hash, EnumCount, EnumIter)]
#[repr(u8)]
pub enum Modifier {
    SchoolClosure = 0,
    CommunityMobilityReduction,
    WorkplaceMobilityReduction,
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

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct ModifierSpecification {
    home: Option<[f64; SETTING_COUNT]>,
    school: Option<[f64; SETTING_COUNT]>,
    work: Option<[f64; SETTING_COUNT]>,
    community: Option<[f64; SETTING_COUNT]>,
}

impl ModifierSpecification {
    pub fn validate(&self) -> Result<(), ModelError> {
        let categories = [
            (SettingCategory::Home, self.home),
            (SettingCategory::School, self.school),
            (SettingCategory::Work, self.work),
            (SettingCategory::Community, self.community),
        ];
        for (category, modifier) in categories.iter() {
            if let Some(modifier) = modifier {
                let sum: f64 = modifier.iter().sum();
                if (sum - 1.0).abs() > 1e-6 {
                    return Err(ModelError::ModelError(format!(
                        "Modifier specification for {:?} does not sum to 1.0",
                        category
                    )));
                }
                for &value in modifier.iter() {
                    if !(0.0..=1.0).contains(&value) {
                        return Err(ModelError::ModelError(format!(
                            "Modifier specification for {:?} values must be between 0 and 1",
                            category
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Intervention {
    modifier: Modifier,
    geography: Geography,
    acceptance_probability: f64,
    activation_time: f64,
    duration: Option<f64>,
    override_modifiers: Option<ModifierSpecification>,
}

impl Intervention {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !(0.0..=1.0).contains(&self.acceptance_probability) {
            return Err(ModelError::ModelError(
                "Intervention acceptance probability must be between 0 and 1".to_string(),
            ));
        }
        if self.activation_time < 0.0 {
            return Err(ModelError::ModelError(
                "Intervention activation time must be non-negative".to_string(),
            ));
        }
        if let Some(duration) = self.duration
            && duration <= 0.0
        {
            return Err(ModelError::ModelError(
                "Intervention duration must be positive".to_string(),
            ));
        }
        if let Some(overrides) = &self.override_modifiers {
            overrides.validate()?;
        }
        Ok(())
    }
    pub fn validate_overlap(&self, other: &Intervention) -> Result<(), ModelError> {
        if self.modifier == other.modifier && self.geography == other.geography {
            let self_end_time = self.activation_time + self.duration.unwrap_or(f64::INFINITY);
            let other_end_time = other.activation_time + other.duration.unwrap_or(f64::INFINITY);
            if (self.activation_time < other_end_time) && (other.activation_time < self_end_time) {
                return Err(ModelError::ModelError(format!(
                    "Interventions {:?} and {:?} overlap in time and geography",
                    self, other
                )));
            }
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

pub trait SchoolClosureContextExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt + ContextSettingExt
{
    fn setup_intervention_triggers(&mut self, intervention: Intervention) {
        let start_trigger =
            TimeTrigger::at_phase(intervention.activation_time, ExecutionPhase::Last).emit_value(
                InterventionEvent {
                    active: true,
                    intervention,
                },
            );
        self.register_trigger(start_trigger);

        if let Some(duration) = intervention.duration {
            let end_trigger = TimeTrigger::at_phase(
                intervention.activation_time + duration,
                ExecutionPhase::Last,
            )
            .emit_value(InterventionEvent {
                active: false,
                intervention,
            });
            self.register_trigger(end_trigger);
        }
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
        };
        for person in people {
            let accepts = self.sample_bool(InterventionRng, intervention.acceptance_probability);
            let mut current_accepts = self.get_property::<Person, AcceptsIntervention>(person);
            current_accepts.0[intervention.geography][intervention.modifier] = accepts;
            self.set_property(person, current_accepts);
        }
    }

    fn register_intervention_itinerary_modifier(
        &mut self,
        intervention: Intervention,
    ) -> Result<(), ModelError> {
        let itinerary_modifier = self.define_intervention_itinerary_modifier(intervention);
        match (intervention.modifier, intervention.geography) {
            (Modifier::SchoolClosure, Geography::State(_)) => {
                self.setup_itinerary_modifer(
                    AcceptsSchoolClosureState(true),
                    itinerary_modifier,
                    SettingMembershipChange::NoChange,
                );
            }
            (Modifier::SchoolClosure, Geography::County(_)) => {
                self.setup_itinerary_modifer(
                    AcceptsSchoolClosureCounty(true),
                    itinerary_modifier,
                    SettingMembershipChange::NoChange,
                );
            }
            (Modifier::WorkplaceMobilityReduction, Geography::State(_)) => {
                self.setup_itinerary_modifer(
                    AcceptsWorkMobilityState(true),
                    itinerary_modifier,
                    SettingMembershipChange::Active,
                );
            }
            (Modifier::WorkplaceMobilityReduction, Geography::County(_)) => {
                self.setup_itinerary_modifer(
                    AcceptsWorkMobilityCounty(true),
                    itinerary_modifier,
                    SettingMembershipChange::Active,
                );
            }
            (Modifier::CommunityMobilityReduction, Geography::State(_)) => {
                self.setup_itinerary_modifer(
                    AcceptsCommunityMobilityState(true),
                    itinerary_modifier,
                    SettingMembershipChange::NoChange,
                );
            }
            (Modifier::CommunityMobilityReduction, Geography::County(_)) => {
                self.setup_itinerary_modifer(
                    AcceptsCommunityMobilityCounty(true),
                    itinerary_modifier,
                    SettingMembershipChange::NoChange,
                );
            }
        }
        Ok(())
    }
    fn remove_intervention_itinerary_modifier(
        &mut self,
        intervention: Intervention,
    ) -> Result<(), ModelError> {
        match (intervention.modifier, intervention.geography) {
            (Modifier::SchoolClosure, Geography::State(_)) => {
                self.remove_itinerary_modifier_by_property(AcceptsSchoolClosureState(true));
            }
            (Modifier::SchoolClosure, Geography::County(_)) => {
                self.remove_itinerary_modifier_by_property(AcceptsSchoolClosureCounty(true));
            }
            (Modifier::WorkplaceMobilityReduction, Geography::State(_)) => {
                self.remove_itinerary_modifier_by_property(AcceptsWorkMobilityState(true));
            }
            (Modifier::WorkplaceMobilityReduction, Geography::County(_)) => {
                self.remove_itinerary_modifier_by_property(AcceptsWorkMobilityCounty(true));
            }
            (Modifier::CommunityMobilityReduction, Geography::State(_)) => {
                self.remove_itinerary_modifier_by_property(AcceptsCommunityMobilityState(true));
            }
            (Modifier::CommunityMobilityReduction, Geography::County(_)) => {
                self.remove_itinerary_modifier_by_property(AcceptsCommunityMobilityCounty(true));
            }
        }
        Ok(())
    }

    fn is_intervention_active(&self, modifier: Modifier, geography: Geography) -> bool {
        let data = self.get_data(InterventionDataPlugin);
        data.is_intervention_active(modifier, geography)
    }

    fn define_intervention_itinerary_modifier(
        &self,
        intervention: Intervention,
    ) -> ItineraryTransitionMatrix {
        let default_modifers = self.get_params().default_modifiers.clone();
        let modifier_params = default_modifers
            .get(&intervention.modifier)
            .expect("Modifier parameters not found");

        let mut matrix = [[0.0; SETTING_COUNT]; SETTING_COUNT];

        for (target, source) in matrix
            .iter_mut()
            .zip(collapse_modifier_specification(modifier_params))
        {
            if let Some(source) = source {
                *target = source;
            }
        }

        if let Some(overrides) = intervention.override_modifiers {
            for (target, source) in matrix
                .iter_mut()
                .zip(collapse_modifier_specification(&overrides))
            {
                if let Some(source) = source {
                    *target = source;
                }
            }
        }

        let acceptance_function: Option<AcceptanceFunction> =
            Some(Box::new(move |context, _person| {
                match intervention.geography {
                    Geography::State(_) => context
                        .is_intervention_active(intervention.modifier, intervention.geography),
                    Geography::County(code) => {
                        !context.is_intervention_active(
                            intervention.modifier,
                            Geography::State(code.state_code()),
                        ) && context
                            .is_intervention_active(intervention.modifier, intervention.geography)
                    }
                }
            }));
        // Interventions have different effects on location and activity
        match intervention.modifier {
            Modifier::SchoolClosure => {
                create_itinerary_transition_matrix(None, Some(matrix), acceptance_function)
            }
            Modifier::WorkplaceMobilityReduction => {
                create_itinerary_transition_matrix(None, Some(matrix), acceptance_function)
            }
            Modifier::CommunityMobilityReduction => {
                create_itinerary_transition_matrix(Some(matrix), None, acceptance_function)
            }
        }
    }
}
impl SchoolClosureContextExt for Context {}

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

fn collapse_modifier_specification(
    modifier_spec: &ModifierSpecification,
) -> [Option<[f64; SETTING_COUNT]>; SETTING_COUNT] {
    let mut matrix: [Option<[f64; SETTING_COUNT]>; SETTING_COUNT] = [None; SETTING_COUNT];
    if let Some(home) = modifier_spec.home {
        matrix[SettingCategory::Home] = Some(home);
    }
    if let Some(school) = modifier_spec.school {
        matrix[SettingCategory::School] = Some(school);
    }
    if let Some(work) = modifier_spec.work {
        matrix[SettingCategory::Work] = Some(work);
    }
    if let Some(community) = modifier_spec.community {
        matrix[SettingCategory::Community] = Some(community);
    }
    matrix
}

#[cfg(test)]
mod test {
    use crate::{
        Age,
        itinerary_manager::ContextItineraryModifierExt,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::parser::parse_fips_school_id,
        settings::SettingCode,
    };
    use ixa::{HashMap, assert_almost_eq};
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
                    ModifierSpecification {
                        home: None,
                        school: Some([1.0, 0.0, 0.0, 0.0]),
                        work: None,
                        community: None,
                    },
                ),
                (
                    Modifier::WorkplaceMobilityReduction,
                    ModifierSpecification {
                        home: None,
                        school: None,
                        work: Some([1.0, 0.0, 0.0, 0.0]),
                        community: None,
                    },
                ),
                (
                    Modifier::CommunityMobilityReduction,
                    ModifierSpecification {
                        home: None,
                        school: None,
                        work: None,
                        community: Some([1.0, 0.0, 0.0, 0.0]),
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
        let g1 = Geography::State(1);
        let school_closure = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 1.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        let work_place_mobility_reduction = Intervention {
            modifier: Modifier::WorkplaceMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 3.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        let community_mobility_reduction = Intervention {
            modifier: Modifier::CommunityMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 5.0,
            duration: Some(1.0),
            override_modifiers: None,
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
        let g1 = Geography::State(school_code.0.state_code());
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let school_closure = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 1.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        let work_place_mobility_reduction = Intervention {
            modifier: Modifier::WorkplaceMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 3.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        let community_mobility_reduction = Intervention {
            modifier: Modifier::CommunityMobilityReduction,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 5.0,
            duration: Some(1.0),
            override_modifiers: None,
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
    fn test_intervention_override_modifier() {
        let mut context = setup();
        let school_code = make_school_id(b"16037960200002");
        let g1 = Geography::State(school_code.0.state_code());
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        let intervention = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: g1,
            acceptance_probability: 1.0,
            activation_time: 1.0,
            duration: Some(1.0),
            override_modifiers: Some(ModifierSpecification {
                home: None,
                school: Some([0.5, 0.0, 0.0, 0.5]),
                work: None,
                community: None,
            }),
        };
        context.setup_intervention_triggers(intervention);
        context.setup_intervention_trigger_event_subscription();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.add_plan(0.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.add_plan(1.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.55, 0.0, 0.0, 0.45]);
        });
        context.add_plan(3.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.execute();
    }

    #[test]
    fn test_intervention_acceptance_probability() {
        let mut context = setup();
        let acceptance = Rc::new(RefCell::new(0));
        let acceptance_clone: Rc<RefCell<usize>> = Rc::clone(&acceptance);
        let school_code = make_school_id(b"16037960200002");
        let g1 = Geography::State(school_code.0.state_code());
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
            activation_time: 1.0,
            duration: Some(1.0),
            override_modifiers: None,
        };
        context.setup_intervention_triggers(intervention);
        context.setup_intervention_trigger_event_subscription();

        context.add_plan(1.5, move |context| {
            let people = context.get_entity_iterator::<Person>();
            for person in people {
                let accepts = context.get_property::<Person, AcceptsIntervention>(person);
                if accepts.0[g1][Modifier::SchoolClosure] {
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
    fn test_modifier_specification_validation() {
        // Valid modifier specification - all values sum to 1.0 and are in [0, 1]
        let valid_modifier = ModifierSpecification {
            home: Some([0.3, 0.2, 0.3, 0.2]),
            school: Some([1.0, 0.0, 0.0, 0.0]),
            work: None,
            community: None,
        };
        assert!(valid_modifier.validate().is_ok());

        // Invalid - values don't sum to 1.0
        let invalid_sum = ModifierSpecification {
            home: Some([0.3, 0.2, 0.3, 0.1]),
            school: None,
            work: None,
            community: None,
        };
        assert!(invalid_sum.validate().is_err());

        // Invalid - values outside [0, 1]
        let invalid_range = ModifierSpecification {
            home: Some([1.5, -0.5, 0.0, 0.0]),
            school: None,
            work: None,
            community: None,
        };
        assert!(invalid_range.validate().is_err());

        // Valid - all categories with valid values
        let all_categories = ModifierSpecification {
            home: Some([0.5, 0.25, 0.25, 0.0]),
            school: Some([0.0, 1.0, 0.0, 0.0]),
            work: Some([0.2, 0.3, 0.4, 0.1]),
            community: Some([0.1, 0.1, 0.1, 0.7]),
        };
        assert!(all_categories.validate().is_ok());

        // Valid - all None
        let all_none = ModifierSpecification {
            home: None,
            school: None,
            work: None,
            community: None,
        };
        assert!(all_none.validate().is_ok());
    }

    #[test]
    fn test_intervention_validation() {
        // Valid intervention with all fields
        let valid_intervention = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(1),
            acceptance_probability: 0.95,
            activation_time: 10.0,
            duration: Some(5.0),
            override_modifiers: Some(ModifierSpecification {
                home: Some([0.6, 0.1, 0.2, 0.1]),
                school: None,
                work: None,
                community: None,
            }),
        };
        assert!(valid_intervention.validate().is_ok());

        // Invalid - acceptance probability > 1.0
        let invalid_probability_high = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(1),
            acceptance_probability: 1.5,
            activation_time: 10.0,
            duration: Some(5.0),
            override_modifiers: None,
        };
        assert!(invalid_probability_high.validate().is_err());

        // Invalid - acceptance probability < 0.0
        let invalid_probability_low = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(1),
            acceptance_probability: -0.1,
            activation_time: 10.0,
            duration: Some(5.0),
            override_modifiers: None,
        };
        assert!(invalid_probability_low.validate().is_err());

        // Invalid - negative activation time
        let invalid_activation = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(1),
            acceptance_probability: 0.5,
            activation_time: -1.0,
            duration: Some(5.0),
            override_modifiers: None,
        };
        assert!(invalid_activation.validate().is_err());

        // Invalid - non-positive duration
        let invalid_duration = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(1),
            acceptance_probability: 0.5,
            activation_time: 10.0,
            duration: Some(0.0),
            override_modifiers: None,
        };
        assert!(invalid_duration.validate().is_err());

        // Invalid - invalid override modifiers
        let invalid_overrides = Intervention {
            modifier: Modifier::SchoolClosure,
            geography: Geography::State(1),
            acceptance_probability: 0.5,
            activation_time: 10.0,
            duration: Some(5.0),
            override_modifiers: Some(ModifierSpecification {
                home: Some([0.5, 0.5, 0.5, 0.5]), // Sum is 2.0, not 1.0
                school: None,
                work: None,
                community: None,
            }),
        };
        assert!(invalid_overrides.validate().is_err());
    }
}
