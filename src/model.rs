use ixa::{ExecutionPhase, prelude::*};
use crate::settings;
#[allow(unused_imports)]
use crate::{population_loader, setting_loader};

pub fn initialize_model(context: &mut Context, seed: u64, max_time: f64, population_loader_method: usize) -> Result<(), IxaError> {
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
    if population_loader_method == 5 {
        setting_loader::init(context)?;
    }
    population_loader::init(context, population_loader_method)?;


    Ok(())
}
