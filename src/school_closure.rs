use ixa::prelude::*;

use crate::{ContextParametersExt, itinerary_manager::ItineraryModifier, policy::{ContextPolicyExt, Policy, TriggerEvent, TriggerType}, population_loader::{ItineraryRatios, Student, Worker}, settings::{ContextSettingExt, SETTING_COUNT}};

static SCHOOL_CLOSURE_RATIOS: [f64;SETTING_COUNT] = [0.75, 0.0, 0.0, 0.25];
static SCHOOL_CLOSURE_WITH_WORK_RATIOS: [f64;SETTING_COUNT] = [0.5, 0.25, 0.0, 0.25];


pub trait ContextSchoolClosureExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt + ContextPolicyExt{
    fn add_school_closure_policy(&mut self) {
        let school_closure_itinerary_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: SCHOOL_CLOSURE_RATIOS,
            }
        };

        let school_closure_work_itinerary_modifier = ItineraryModifier {
            ranking: 1,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: SCHOOL_CLOSURE_WITH_WORK_RATIOS,
            }
        };

        let trigger_school_closure = TriggerEvent {
            trigger_type: TriggerType::SchoolClosure,
            active: true,
        };

        let trigger_school_opening = TriggerEvent {
            trigger_type: TriggerType::SchoolClosure,
            active: false,
        };

        let school_closure_policy = Policy::Closure {
            trigger_event: trigger_school_closure,
            collection_of_people: (Student(true), Worker(false)),
            itinerary_modifier: school_closure_itinerary_modifier.clone(),
        };

        let school_closure_work_policy = Policy::Closure {
            trigger_event: trigger_school_closure,
            collection_of_people: (Student(true), Worker(true)),
            itinerary_modifier: school_closure_work_itinerary_modifier.clone(),
        };

        self.add_policy(school_closure_policy);
        self.add_policy(school_closure_work_policy);

        self.add_plan(1.0, move |context| {
            context.emit_event(trigger_school_closure);
        });

        self.add_plan(40.0, move |context| {
            context.emit_event(trigger_school_opening);
        });

    }
}
impl ContextSchoolClosureExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.add_school_closure_policy();
    Ok(())
}