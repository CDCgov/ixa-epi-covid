use ixa::{
    Context, PluginContext, define_data_plugin,
    prelude_for_plugins::{IxaEvent, PropertyChangeEvent},
};

use crate::{settings::Person, symptom_status_manager::SymptomStatus};

#[derive(IxaEvent, Copy, Clone, Debug)]
pub struct HospitalizationThresholdEvent {
    pub above_threshold: bool,
}

/// An index of settings as represented by their setting codes.
#[derive(Default)]
pub struct HospitalizationData {
    hospitalization_counter: usize,
    threshold: usize,
}

impl HospitalizationData {
    pub fn new() -> Self {
        Self {
            hospitalization_counter: 0,
            threshold: 0,
        }
    }

    pub fn increment_hospitalization_counter(&mut self) {
        self.hospitalization_counter += 1;
    }

    pub fn decrement_hospitalization_counter(&mut self) {
        if self.hospitalization_counter > 0 {
            self.hospitalization_counter -= 1;
        }
    }

    pub fn get_hospitalization_counter(&self) -> usize {
        self.hospitalization_counter
    }

    pub fn set_threshold(&mut self, threshold: usize) {
        self.threshold = threshold;
    }

    pub fn get_threshold(&self) -> usize {
        self.threshold
    }
}

define_data_plugin!(HospitalizationDataPlugin, HospitalizationData, |_context| {
    HospitalizationData::new()
});

pub trait ContextCustomEventExt: PluginContext {
    fn emit_hospitalization_threshold_event(&mut self, above_threshold: bool) {
        println!(
            "Emitting HospitalizationThresholdEvent with above_threshold {}",
            above_threshold
        );
        self.emit_event(HospitalizationThresholdEvent { above_threshold });
    }
    fn increment_hospitalization_counter(&mut self) {
        let data = self.get_data_mut(HospitalizationDataPlugin);
        data.increment_hospitalization_counter();
    }

    fn decrement_hospitalization_counter(&mut self) {
        let data = self.get_data_mut(HospitalizationDataPlugin);
        data.decrement_hospitalization_counter();
    }

    fn get_hospitalization_counter(&self) -> usize {
        let data = self.get_data(HospitalizationDataPlugin);
        data.get_hospitalization_counter()
    }

    fn set_hospitalization_threshold(&mut self, threshold: usize) {
        let data = self.get_data_mut(HospitalizationDataPlugin);
        data.set_threshold(threshold);
    }

    fn get_hospitalization_threshold(&self) -> usize {
        let data = self.get_data(HospitalizationDataPlugin);
        data.get_threshold()
    }

    fn subscribe_to_hospitalization(&mut self) {
        self.subscribe_to_event::<PropertyChangeEvent<Person, SymptomStatus>>(
            move |context, event| {
                if let SymptomStatus::Critical = event.current {
                    context.increment_hospitalization_counter();
                    if context.get_hospitalization_counter()
                        >= context.get_hospitalization_threshold()
                    {
                        context.emit_hospitalization_threshold_event(true);
                    }
                }
                if let SymptomStatus::Critical = event.previous {
                    context.decrement_hospitalization_counter();
                    if context.get_hospitalization_counter()
                        <= context.get_hospitalization_threshold()
                    {
                        context.emit_hospitalization_threshold_event(false);
                    }
                }
            },
        );
    }
}
impl ContextCustomEventExt for Context {}
