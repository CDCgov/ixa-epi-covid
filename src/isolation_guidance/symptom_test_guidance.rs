use ixa::{Context, prelude::*};
use serde::Serialize;

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    infectiousness_manager::InfectionStatus,
    intervention_manager::Modifier,
    isolation_guidance::{EligibleForIsolationStatus, SymptomTestGuidance},
    settings::{Person, PersonId},
    symptom_status_manager::PresenceOfSymptoms,
};

define_rng!(SymptomTestGuidanceRng);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct NegativeTests(pub usize);
impl_property!(NegativeTests, Person, default_const = NegativeTests(0));
pub trait ContextIsolationGuidanceExt: PluginContext + ContextRandomExt {
    fn start_isolation(&mut self, person_id: PersonId, symptom_test_guidance: SymptomTestGuidance) {
        self.add_plan(
            self.get_current_time() + symptom_test_guidance.delay_symptoms_to_isolation,
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

    fn test_person(&mut self, person_id: PersonId, symptom_test_guidance: SymptomTestGuidance) {
        println!(
            "number of negative tests: {:?}",
            self.get_property::<Person, NegativeTests>(person_id).0
        );
        if self.get_property::<Person, InfectionStatus>(person_id) == InfectionStatus::Infectious {
            if !self.sample_bool(
                SymptomTestGuidanceRng,
                symptom_test_guidance.test_sensitivity,
            ) {
                let negative_tests = self.get_property::<Person, NegativeTests>(person_id);
                self.set_property(person_id, NegativeTests(negative_tests.0 + 1));
            }
        } else {
            let negative_tests = self.get_property::<Person, NegativeTests>(person_id);
            self.set_property(person_id, NegativeTests(negative_tests.0 + 1));
        }
    }

    fn schedule_future_test(
        &mut self,
        person_id: PersonId,
        symptom_test_guidance: SymptomTestGuidance,
    ) {
        println!("Scheduling future test for person: {:?}", person_id);
        self.add_plan(
            self.get_current_time() + symptom_test_guidance.test_interval,
            move |context| {
                context.test_person(person_id, symptom_test_guidance);
                if context.get_property::<Person, NegativeTests>(person_id).0
                    < symptom_test_guidance.tests_post_symptoms
                {
                    context.schedule_future_test(person_id, symptom_test_guidance);
                }
            },
        );
    }

    fn plan_testing(
        &mut self,
        person_id: PersonId,
        symptom_test_guidance: SymptomTestGuidance,
    ) -> Result<(), ModelError> {
        println!("planning testing for person: {:?}", person_id);
        self.test_person(person_id, symptom_test_guidance);
        if self.get_property::<Person, NegativeTests>(person_id).0
            < symptom_test_guidance.tests_post_symptoms
        {
            self.schedule_future_test(person_id, symptom_test_guidance);
        }
        Ok(())
    }

    fn setup_isolation_guidance_event_sequence(
        &mut self,
        symptom_test_guidance: SymptomTestGuidance,
    ) {
        self.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, PresenceOfSymptoms>| {
                if event.current.0 {
                    context.start_isolation(event.entity_id, symptom_test_guidance);
                } else if event.previous.0 {
                    //individuals transition from presenting with symptoms to not presenting with symptoms
                    context
                        .plan_testing(event.entity_id, symptom_test_guidance)
                        .unwrap();
                }
            },
        );

        self.subscribe_to_event(
            move |context, event: PropertyChangeEvent<Person, NegativeTests>| {
                if event.current.0 == symptom_test_guidance.tests_post_symptoms {
                    context.set_property(event.entity_id, EligibleForIsolationStatus(false));
                    context.set_property(event.entity_id, NegativeTests(0));
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
    let symptom_test_guidance = default_modifiers
        .get(&Modifier::IsolationGuidance)
        .expect("IsolationGuidance parameters not found")
        .get_symptom_test_guidance()
        .expect("SymptomTestGuidance incorrectly specified");
    context.setup_isolation_guidance_event_sequence(symptom_test_guidance);
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::{
        Age, Params,
        intervention_manager::{InterventionModifierSpecification, Modifier},
        isolation_guidance::IsolationGuidanceParameters,
        parameters::{GlobalParams, SettingProperties},
        settings::SettingCategory,
        symptom_status_manager::SymptomData,
    };
    use ixa::{HashMap, assert_almost_eq};
    use std::{cell::RefCell, panic, rc::Rc};

    use super::*;

    fn setup(isolation_guidance: SymptomTestGuidance) -> Context {
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
                        isolation_guidance: IsolationGuidanceParameters::SymptomTestGuidance(
                            isolation_guidance,
                        ),
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
    fn test_symptom_test_guidance_event_sequence() {
        // 1. Create a new person
        // 2. Keep track of the time of symptom onset and duration
        // 3. Assert that start of isolation is the same as symptom onset + isolation delay
        // 4. Assert that the end of isolation is at the minimum isolation duration
        let symptom_test_guidance = SymptomTestGuidance {
            tests_post_symptoms: 2,
            test_interval: 1.0,
            test_sensitivity: 1.0,
            delay_symptoms_to_isolation: 1.0,
        };

        let mut context = setup(symptom_test_guidance);
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        super::init(&mut context).unwrap();

        let policy_ran_flag = Rc::new(RefCell::new(false));
        let policy_ran_flag_clone = Rc::clone(&policy_ran_flag);
        let negative_test_flag = Rc::new(RefCell::new(false));
        let negative_test_flag_clone = Rc::clone(&negative_test_flag);

        context.subscribe_to_event::<PropertyChangeEvent<Person, EligibleForIsolationStatus>>(
            move |context, event| {
                let symptom_data = context.get_property::<Person, SymptomData>(event.entity_id);
                println!("Event received: {:?}", event.current);
                if event.current.0 {
                    let symptom_onset = match symptom_data {
                        SymptomData::Mild { mild_time, .. } => mild_time,
                        _ => panic!("Unexpected symptom data"),
                    };
                    assert_almost_eq!(
                        symptom_onset + symptom_test_guidance.delay_symptoms_to_isolation,
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

                    assert!(symptom_resolution >= symptom_onset);
                    assert_eq!(
                        context.get_current_time(),
                        symptom_resolution + symptom_test_guidance.test_interval
                    );
                    *policy_ran_flag_clone.borrow_mut() = true;
                }
            },
        );

        context.subscribe_to_event::<PropertyChangeEvent<Person, NegativeTests>>(
            move |_context, event| {
                if event.current.0 == 2 {
                    *negative_test_flag_clone.borrow_mut() = true;
                }
            },
        );

        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(p1, SymptomData::Mild { mild_time: 1.0 });
        });

        context.add_plan(3.0, move |context| {
            println!("Setting symptom data to resolved at time 3.0");
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
        assert!(*negative_test_flag.borrow());
    }
}
