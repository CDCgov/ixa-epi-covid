use ixa::{
    ExecutionPhase, impl_derived_property,
    prelude::*,
    triggers::{PeriodicTimeTrigger, TriggerCriterion},
};
use serde::Serialize;

use crate::{
    ContextParametersExt, Params,
    itinerary_modifiers::ItineraryTransitionMatrix,
    settings::{Itinerary, Person, SettingCategory},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct GoesToSchool(pub bool);

impl_derived_property!(GoesToSchool, Person, [Itinerary], [], |itinerary| {
    GoesToSchool(itinerary.setting_ids[SettingCategory::School].is_some())
});

pub trait ContextCalendarExt: PluginContext + ContextEntitiesExt + ContextParametersExt {
    fn add_weekends(
        &mut self,
        weekend_itinerary_modifier: ItineraryTransitionMatrix,
        max_time: f64,
    ) {
        let trigger = PeriodicTimeTrigger::every(7.0)
            .with_phase(ExecutionPhase::Last)
            .start_with_delay(3.0)
            .emit_value(Weekend);
    }
}
impl ContextCalendarExt for Context {}

pub fn init(context: &mut Context) {
    let Params {
        max_time, weekends, ..
    } = context.get_params().clone();
    if let Some(weekends_itinerary_modifier) = weekends.itinerary_modifier {
        context.add_weekends(weekends_itinerary_modifier, max_time);
    }
}
