use ixa::prelude::*;

use crate::{ContextParametersExt, itinerary_manager::ItineraryModifier, policy::{ContextPolicyExt, Policy, TriggerEvent, TriggerType}, population_loader::{Alive, ItineraryRatios}, settings::{ContextSettingExt, SETTING_COUNT}};

static WEEKEND_RATIOS: [f64; SETTING_COUNT] = [0.5, 0.0, 0.0, 0.5];

// define_entity!(Calendar);

// #[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
// pub struct Weekend(pub bool);
// impl_property!(Weekend, Calendar, default_const = Weekend(false));

// #[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
// pub struct SummerBreak(pub bool);
// impl_property!(SummerBreak, Calendar, default_const = SummerBreak(false));


pub trait ContextSchoolClosureExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt + ContextPolicyExt{
    fn add_weekend_itinerary_change(&mut self) {
        let weekend_itinerary_modifier = ItineraryModifier {
            ranking: 2,
            itinerary_ratios: ItineraryRatios {
                itinerary_ratios: WEEKEND_RATIOS,
            }
        };

        let weekend_trigger_close = TriggerEvent {
            trigger_type: TriggerType::Weekend,
            active: true,
        };


        let weekend_policy = Policy::Closure {
            trigger_event: weekend_trigger_close,
            collection_of_people: (Alive(true),),
            itinerary_modifier: weekend_itinerary_modifier.clone(),
        };

        self.add_policy(weekend_policy);
        self.schedule_weekend_events();
    }

    fn schedule_weekend_events(&mut self) {
        let weekend_trigger_close = TriggerEvent {
            trigger_type: TriggerType::Weekend,
            active: true,
        };

        let weekend_trigger_open = TriggerEvent {
            trigger_type: TriggerType::Weekend,
            active: false,
        };
        for day in (2..100).step_by(7) {
            self.add_plan(day as f64, move |context| {
                context.emit_event(weekend_trigger_close);
            });
            self.add_plan((day + 2) as f64, move |context| {
                context.emit_event(weekend_trigger_open);
            });
        }
    }

}
impl ContextSchoolClosureExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.add_weekend_itinerary_change();
    Ok(())
}