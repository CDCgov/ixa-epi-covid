use std::path::PathBuf;

use ixa::{ExecutionPhase, prelude::*};

use crate::{
    abort_run, error::ModelError, infection_importation, infection_propagation_loop,
    intervention_manager, parameters, population_loader, reports, school_calendar, settings,
    symptom_status_manager,
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
    println!("Settings initialized");
    population_loader::init(context, synth_population_override)?;
    println!("Population loaded");
    symptom_status_manager::init(context)?;
    println!("Symptom status manager initialized");
    infection_propagation_loop::init(context)?;
    println!("Infection propagation loop initialized");
    infection_importation::init(context)?;
    println!("Infection importation initialized");
    school_calendar::init(context);
    println!("School calendar initialized");
    intervention_manager::init(context)?;
    println!("School closure initialized");
    reports::init(context)?;
    println!("Reports initialized");
    abort_run::init(context);
    println!("Setup complete");

    Ok(())
}
