use ixa::{
    Context, PluginContext,
    prelude_for_plugins::{IxaEvent, PropertyChangeEvent},
};

use crate::{infectiousness_manager::InfectionStatus, policy::TriggerEventTrait, settings::Person};

#[derive(IxaEvent, Copy, Clone, Debug)]
pub struct TimeEvent {
    pub time: f64,
}

impl TriggerEventTrait for TimeEvent {
    fn get_trigger_value(&self) -> f64 {
        self.time
    }
}

#[derive(IxaEvent, Copy, Clone, Debug)]
pub struct InfectionEvent {
    pub value: f64,
}

impl TriggerEventTrait for InfectionEvent {
    fn get_trigger_value(&self) -> f64 {
        self.value
    }
}

pub trait ContextCustromEventExt: PluginContext {
    fn emit_time_event(&mut self, time: f64) {
        println!("Scheduling TimeEvent at time {}", time);
        self.add_plan(time, move |context| {
            println!("Emitting TimeEvent at time {}", time);
            context.emit_event(TimeEvent { time });
        });
    }

    fn emit_infection_event(&mut self, value: f64) {
        println!("Emitting InfectionEvent with value {}", value);
        self.emit_event(InfectionEvent { value });
    }

    fn subscribe_to_infections(&mut self) {
        self.subscribe_to_event::<PropertyChangeEvent<Person, InfectionStatus>>(
            move |context, event| {
                if let InfectionStatus::Infectious = event.current {
                    context.emit_infection_event(1.0);
                }
            },
        );
    }
}
impl ContextCustromEventExt for Context {}
