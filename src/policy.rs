use ixa::{entity::query::Query, prelude::*};
use serde::{Serialize};

use crate::{ContextParametersExt, itinerary_manager::{ContextItineraryModifierExt, ItineraryModifier}, population_loader::{Alive, ItineraryRatios, Person, Student, Worker}, settings::{ContextSettingExt, SettingCode}};

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct TimeTrigger {
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct PrevalenceTrigger {
    pub setting_code: SettingCode,
    pub prevalence_threshold: f64,
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub enum PolicyTrigger {
    Time(TimeTrigger),
    Prevalence(PrevalenceTrigger),
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub enum Policy<T: Query<Person>>{
    Closure {
        trigger: PolicyTrigger,
        collection_of_people: T,
        itinerary_modifier: ItineraryModifier,
    },
}

pub trait ContextPolicyExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextItineraryModifierExt + ContextSettingExt {
    fn add_policy<T: Query<Person>>(&mut self, policy: Policy<T>) {
        match policy {
            Policy::Closure { trigger, collection_of_people, itinerary_modifier } => {
                match trigger {
                    PolicyTrigger::Time(time_trigger) => {
                        let people: Vec<_> = self.query_result_iterator(collection_of_people.clone()).collect();
                        for person in people{
                            self.add_plan(time_trigger.start_time, move |context| {
                                context.register_itinerary_modifier(person, itinerary_modifier);
                            });
                            self.add_plan(time_trigger.end_time, move |context| {
                                context.remove_itinerary_modifier(person, itinerary_modifier);
                            });
                        }
                    },
                    #[allow(unused_variables)]
                    PolicyTrigger::Prevalence(prevalence_trigger) => {
                        // Implement prevalence-based trigger logic here
                    },
                }
            },
        }
    }

    fn add_periodic_policy<T: Query<Person>>(&mut self, policy: Policy<T>, period: f64, offset: f64) {
        match policy {
            Policy::Closure { trigger, collection_of_people, itinerary_modifier } => {
                match trigger {
                    PolicyTrigger::Time(time_trigger) => {
                        let people: Vec<_> = self.query_result_iterator(collection_of_people.clone()).collect();
                        for person in people{
                            let start_time = time_trigger.start_time;
                            let end_time = time_trigger.end_time;
                            for time in (start_time as usize..end_time as usize).step_by(period as usize) {
                                self.add_plan(time as f64, move |context| {
                                    context.register_itinerary_modifier(person, itinerary_modifier);
                                });
                                self.add_plan(time as f64 + offset, move |context| {
                                    context.remove_itinerary_modifier(person, itinerary_modifier);
                                });
                            }
                        }
                    },
                    #[allow(unused_variables)]
                    PolicyTrigger::Prevalence(prevalence_trigger) => {
                        // Implement prevalence-based trigger logic here
                    },
                }
            },
        }
    }


    fn add_school_closure_policy(&mut self) {
        let school_closure_itinerary_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.75, 0.0, 0.0, 0.25],
            }
        };

        let school_closure_work_itinerary_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.5, 0.25, 0.0, 0.25],
            }
        };

        let school_closure_policy = Policy::Closure {
            trigger: PolicyTrigger::Time(TimeTrigger { start_time: 1.0, end_time: 40.0 }),
            collection_of_people: (Student(true), Worker(false)),
            itinerary_modifier: school_closure_itinerary_modifier.clone(),
        };

        let school_closure_work_policy = Policy::Closure {
            trigger: PolicyTrigger::Time(TimeTrigger { start_time: 1.0, end_time: 40.0 }),
            collection_of_people: (Student(true), Worker(true)),
            itinerary_modifier: school_closure_work_itinerary_modifier.clone(),
        };

        self.add_policy(school_closure_policy);
        self.add_policy(school_closure_work_policy);
    }

    fn add_weekend_policy(&mut self) {
        let weekend_itinerary_modifier = ItineraryModifier {
            ranking: 2,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: [0.5, 0.0, 0.0, 0.5],
            }
        };

        let weekend_policy = Policy::Closure {
            trigger: PolicyTrigger::Time(TimeTrigger { start_time: 1.0, end_time: 100.0 }),
            collection_of_people: (Alive(true),),
            itinerary_modifier: weekend_itinerary_modifier.clone(),
        };

        self.add_periodic_policy(weekend_policy, 7.0, 2.0);
    }

}
impl ContextPolicyExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // context.add_school_closure_policy();
    context.add_weekend_policy();
    Ok(())
}
