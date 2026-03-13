use crate::{
    parameters::{ContextParametersExt, Params},
    population_loader::Person,
    symptom_status_manager::SymptomStatus,
};
use ixa::prelude::*;

pub fn init(context: &mut Context) {
    let &Params {
        first_death_terminates_run,
        ..
    } = context.get_params();
    if first_death_terminates_run {
        context.subscribe_to_event::<PropertyChangeEvent<Person, SymptomStatus>>(
            move |context, event| {
                if event.current == SymptomStatus::Dead {
                    context.add_plan(context.get_current_time() + 1.0, move |context| {
                        context.shutdown();
                    });
                }
            },
        );
    }
}
