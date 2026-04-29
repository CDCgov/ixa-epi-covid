use ixa::{entity::query::Query, prelude::*, prelude_for_plugins::IxaEvent};
use serde::{Serialize};

use crate::{ContextParametersExt, itinerary_manager::{ContextItineraryModifierExt, ItineraryModifier}, population_loader::{Person}, settings::{ContextSettingExt}};


#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub enum TriggerType {
    SchoolClosure,
    Weekend,
}

/// Emitted when a trigger is activated or deactivated.
#[derive(IxaEvent)]
#[allow(clippy::manual_non_exhaustive)]
pub struct TriggerEvent {
    pub active: bool,
    pub trigger_type: TriggerType,
}
// We provide blanket impls for these because the compiler isn't smart enough to know
// this type is always `Copy`/`Clone` if we derive them.
impl Copy for TriggerEvent {}
impl Clone for TriggerEvent {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Clone)]
pub enum Policy<T: Query<Person>>{
    Closure {
        trigger_event: TriggerEvent,
        collection_of_people: T,
        itinerary_modifier: ItineraryModifier,
    },
}

pub trait ContextPolicyExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextItineraryModifierExt + ContextSettingExt {
    fn add_policy<T: Query<Person>>(&mut self, policy: Policy<T>) {
        match policy {
            Policy::Closure { trigger_event, collection_of_people, itinerary_modifier } => {
                // let people: Vec<_> = self.query_result_iterator(collection_of_people.clone()).collect();
                // for person in people{
                //     self.add_plan(trigger_event.start_time, move |context| {
                //         context.register_itinerary_modifier(person, itinerary_modifier);
                //     });
                //     self.add_plan(time_trigger.end_time, move |context| {
                //         context.remove_itinerary_modifier(person, itinerary_modifier);
                //     });
                // }
                self.subscribe_to_event::<TriggerEvent>(move |context, event: TriggerEvent| {
                    if event.trigger_type == trigger_event.trigger_type {
                        let people: Vec<_> = context.query_result_iterator(collection_of_people.clone()).collect();
                        for person in people{
                            if event.active {
                                context.register_itinerary_modifier(person, itinerary_modifier);
                            } else {
                                context.remove_itinerary_modifier(person, itinerary_modifier);
                            }
                        }
                    }
                });
            },
        }
    }


    // fn add_periodic_policy<T: Query<Person>>(&mut self, policy: Policy<T>, period: f64, offset: f64) {
    //     match policy {
    //         Policy::Closure { trigger, collection_of_people, itinerary_modifier } => {
    //             match trigger {
    //                 PolicyTrigger::Time(time_trigger) => {
    //                     let people: Vec<_> = self.query_result_iterator(collection_of_people.clone()).collect();
    //                     for person in people{
    //                         let start_time = time_trigger.start_time;
    //                         let end_time = time_trigger.end_time;
    //                         for time in (start_time as usize..end_time as usize).step_by(period as usize) {
    //                             self.add_plan(time as f64, move |context| {
    //                                 context.register_itinerary_modifier(person, itinerary_modifier);
    //                             });
    //                             self.add_plan(time as f64 + offset, move |context| {
    //                                 context.remove_itinerary_modifier(person, itinerary_modifier);
    //                             });
    //                         }
    //                     }
    //                 },
    //                 #[allow(unused_variables)]
    //                 PolicyTrigger::Prevalence(prevalence_trigger) => {
    //                     // Implement prevalence-based trigger logic here
    //                 },
    //             }
    //         },
    //     }
    // }
}
impl ContextPolicyExt for Context {}

