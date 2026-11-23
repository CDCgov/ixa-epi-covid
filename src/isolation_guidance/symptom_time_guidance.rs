use ixa::prelude::*;

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    intervention_manager::Modifier,
    isolation_guidance::{EligibleForIsolationStatus, SymptomTimeGuidance},
    settings::{Person, PersonId},
    symptom_status_manager::{PresenceOfSymptoms, SymptomData},
};

pub trait ContextIsolationGuidanceExt: PluginContext + ContextRandomExt {
    fn start_isolation(&mut self, person_id: PersonId, symptom_time_guidance: SymptomTimeGuidance) {
        self.add_plan(
            self.get_current_time() + symptom_time_guidance.delay_symptoms_to_isolation,
            move |context| {
                if context
                    .get_property::<Person, PresenceOfSymptoms>(person_id)
                    .0
                {
                    context.set_property(person_id, EligibleForIsolationStatus(true));
                }
            },
        );
    }

    fn end_isolation(
        &mut self,
        person_id: PersonId,
        symptom_time_guidance: SymptomTimeGuidance,
    ) -> Result<(), ModelError> {
        let symptom_data = self.get_property::<Person, SymptomData>(person_id);
        let (symptom_onset, symptom_resolution) = match symptom_data {
            SymptomData::Resolved {
                mild_time,
                resolved_time,
                ..
            } => (mild_time, resolved_time),
            _ => {
                return Err(ModelError::ModelError(
                    "Expected resolved symptom data".to_string(),
                ));
            }
        };

        let symptom_duration = symptom_resolution - symptom_onset;
        let isolation_end_time = symptom_resolution
            + f64::max(
                symptom_time_guidance.minimum_isolation_duration - symptom_duration,
                symptom_time_guidance.post_symptoms_isolation_duration,
            );
        self.add_plan(isolation_end_time, move |context| {
            context.set_property(person_id, EligibleForIsolationStatus(false));
        });
        Ok(())
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        symptom_time_guidance: SymptomTimeGuidance,
    ) {
        self.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, PresenceOfSymptoms>| {
                if event.current.0 {
                    context.start_isolation(event.entity_id, symptom_time_guidance);
                } else if event.previous.0 {
                    //individuals transition from presenting with symptoms to not presenting with symptoms
                    context
                        .end_isolation(event.entity_id, symptom_time_guidance)
                        .unwrap();
                }
            },
        );
    }
}
impl ContextIsolationGuidanceExt for Context {}

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        default_modifiers, ..
    } = context.get_params();

    let symptom_time_guidance = default_modifiers
        .get(&Modifier::IsolationGuidance)
        .expect("SymptomTimeGuidance parameters not found")
        .get_symptom_time_guidance()
        .expect("SymptomTimeGuidance incorrectly specified");

    context.setup_isolation_guidance_event_sequence(symptom_time_guidance);
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        Age,
        intervention_manager::InterventionModifierSpecification,
        parameters::{GlobalParams, SettingProperties},
        settings::SettingCategory,
    };
    use ixa::{HashMap, assert_almost_eq};
    use std::{cell::RefCell, panic, rc::Rc};

    use super::*;

    fn setup(
        post_symptoms_isolation_duration: f64,
        minimum_isolation_duration: f64,
        delay_symptoms_to_isolation: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
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
                            SettingCategory::Home,
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
                        settings: vec![
                            SettingCategory::Home,
                            SettingCategory::Work,
                            SettingCategory::School,
                            SettingCategory::Community,
                        ],
                        post_symptoms_isolation_duration,
                        minimum_isolation_duration,
                        delay_symptoms_to_isolation,
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
    fn test_symptom_time_guidance_event_sequence_short_symptoms() {
        // 1. Create a new person
        // 2. Keep track of the time of symptom onset and duration
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that the end of isolation is at the minimum isolation duration

        let post_symptoms_isolation_duration = 3.0;
        let minimum_isolation_duration = 7.0;
        let delay_symptoms_to_isolation = 1.0;

        let mut context = setup(
            post_symptoms_isolation_duration,
            minimum_isolation_duration,
            delay_symptoms_to_isolation,
        );
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        super::init(&mut context).unwrap();

        let policy_ran_flag = Rc::new(RefCell::new(false));
        let policy_ran_flag_clone = Rc::clone(&policy_ran_flag);

        context.subscribe_to_event::<PropertyChangeEvent<Person, EligibleForIsolationStatus>>(
            move |context, event| {
                let symptom_data = context.get_property::<Person, SymptomData>(event.entity_id);
                if event.current.0 {
                    let symptom_onset = match symptom_data {
                        SymptomData::Mild { mild_time, .. } => mild_time,
                        _ => panic!("Unexpected symptom data"),
                    };
                    assert_almost_eq!(
                        symptom_onset + delay_symptoms_to_isolation,
                        context.get_current_time(),
                        0.0
                    );
                } else {
                    let (symptom_onset, symptom_resolution) = match symptom_data {
                        SymptomData::Resolved {
                            mild_time,
                            resolved_time,
                            ..
                        } => (mild_time, resolved_time),
                        _ => panic!("Unexpected symptom data"),
                    };

                    assert_almost_eq!(
                        symptom_onset + minimum_isolation_duration,
                        context.get_current_time(),
                        0.0
                    );
                    assert!(symptom_resolution >= symptom_onset);
                    assert!(context.get_current_time() >= symptom_resolution);
                    *policy_ran_flag_clone.borrow_mut() = true;
                }
            },
        );
        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
        });

        context.add_plan(3.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 1.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 3.0,
                },
            );
        });
        context.execute();

        // Check that the policy ran
        assert!(*policy_ran_flag.borrow());
    }

    #[test]
    fn test_symptom_time_guidance_event_sequence_long_symptoms() {
        // 1. Create a new person
        // 2. Keep track of the time of symptom onset and duration
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that the end of isolation is at the post-symptoms isolation duration
        // after symptoms resolve

        let post_symptoms_isolation_duration = 3.0;
        let minimum_isolation_duration = 7.0;
        let delay_symptoms_to_isolation = 1.0;

        let mut context = setup(
            post_symptoms_isolation_duration,
            minimum_isolation_duration,
            delay_symptoms_to_isolation,
        );
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        super::init(&mut context).unwrap();

        let policy_ran_flag = Rc::new(RefCell::new(false));
        let policy_ran_flag_clone = Rc::clone(&policy_ran_flag);

        context.subscribe_to_event::<PropertyChangeEvent<Person, EligibleForIsolationStatus>>(
            move |context, event| {
                let symptom_data = context.get_property::<Person, SymptomData>(event.entity_id);
                if event.current.0 {
                    let symptom_onset = match symptom_data {
                        SymptomData::Mild { mild_time, .. } => mild_time,
                        _ => panic!("Unexpected symptom data"),
                    };
                    assert_almost_eq!(
                        symptom_onset + delay_symptoms_to_isolation,
                        context.get_current_time(),
                        0.0
                    );
                } else {
                    let (symptom_onset, symptom_resolution) = match symptom_data {
                        SymptomData::Resolved {
                            mild_time,
                            resolved_time,
                            ..
                        } => (mild_time, resolved_time),
                        _ => panic!("Unexpected symptom data"),
                    };

                    assert_almost_eq!(
                        symptom_resolution + post_symptoms_isolation_duration,
                        context.get_current_time(),
                        0.0
                    );
                    assert!(symptom_resolution >= symptom_onset);
                    assert!(context.get_current_time() >= symptom_resolution);
                    *policy_ran_flag_clone.borrow_mut() = true;
                }
            },
        );
        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
        });

        context.add_plan(10.0, move |context| {
            context.set_property::<Person, SymptomData>(
                p1,
                SymptomData::Resolved {
                    mild_time: 1.0,
                    severe_time: None,
                    critical_time: None,
                    resolved_time: 10.0,
                },
            );
        });
        context.execute();

        // Check that the policy ran
        assert!(*policy_ran_flag.borrow());
    }
}
