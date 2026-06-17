use ixa::{impl_derived_property, prelude::*};
use serde::Serialize;

use crate::{ContextParametersExt, Params, itinerary_manager::ItineraryModifier, policy::{ContextPolicyExt, Policy, PolicyTrigger}, settings::{Person, SettingCategory, SettingIds}};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct GoesToSchool(pub bool);

impl_derived_property!(GoesToSchool, Person, [SettingIds], [], |setting_ids| GoesToSchool(
    setting_ids.setting_ids[SettingCategory::School].is_some()
));

pub trait ContextCalendarExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextPolicyExt
{
    fn add_weekends(&mut self, weekend_itinerary_modifier: ItineraryModifier, max_time: f64){
        let policy: Policy<GoesToSchool, ItineraryModifier> = Policy {
            trigger: PolicyTrigger::PeriodicTimeTrigger {
                interval: 7.0,
                duration: 2.0,
                start_time: 1.0,
                end_time: max_time,
            },
            intervention: weekend_itinerary_modifier,
            group: GoesToSchool(true),
        };
        self.add_policy(policy);
    }
}
impl ContextCalendarExt for Context {}

pub fn init(
    context: &mut Context,
) {
    let Params {
            max_time,
            weekends,
            ..
        } = context.get_params().clone();
    if let Some(weekends_itinerary_modifier) = weekends.itinerary_modifier {
        context.add_weekends(weekends_itinerary_modifier, max_time);
    }
}