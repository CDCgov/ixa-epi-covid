use ixa::{ExecutionPhase, prelude::*};

use crate::{infection_propagation_loop, population_loader, reports, settings};

use crate::{infection_propagation_loop, population_loader, reports, settings};

pub fn initialize_model(context: &mut Context, seed: u64, max_time: f64) -> Result<(), IxaError> {
    // Initialize the random number generator with the provided seed
    context.init_random(seed);

    // Plan to shut down the model at designated maximum run time
    context.add_plan_with_phase(
        max_time,
        move |context| {
            context.shutdown();
        },
        ExecutionPhase::Last,
    );
    settings::init(context);
    population_loader::init(context)?;
    infection_propagation_loop::init(context)?;
    reports::init(context)?;

    Ok(())
}
