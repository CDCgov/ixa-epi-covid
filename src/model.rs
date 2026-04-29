use std::path::PathBuf;

use ixa::{ExecutionPhase, prelude::*};
#[allow(unused_imports)]
use crate::{
    error::ModelError, school_calendar, policy, infection_importation, infection_propagation_loop, itinerary_manager, parameters, population_loader, reports, school_closure, settings, symptom_status_manager
};

pub fn initialize_model(
    context: &mut Context,
    seed: u64,
    max_time: f64,
    synth_population_override: Option<PathBuf>,
) -> Result<(), ModelError> {
    // Initialize the random number generator with the provided seed
    context.init_random(seed);

    parameters::init(context)?;

    // Plan to shut down the model at designated maximum run time
    context.add_plan_with_phase(
        max_time,
        move |context| {
            context.shutdown();
        },
        ExecutionPhase::Last,
    );
    context.set_start_time(-1000.);
    settings::init(context)?;
    info!("Settings initialized");
    population_loader::init(context, synth_population_override)?;
    info!("Population loaded");
    symptom_status_manager::init(context)?;
    info!("Symptom status manager initialized");
    infection_propagation_loop::init(context)?;
    info!("Infection propagation loop initialized");
    infection_importation::init(context)?;
    info!("Infection importation initialized");
    school_closure::init(context)?;
    school_calendar::init(context)?;
    info!("School closure initialized");
    reports::init(context)?;
    info!("Setup complete");

    Ok(())
}
