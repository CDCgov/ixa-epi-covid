use std::path::PathBuf;

use clap::Args;
use epimodel::{ContextParametersExt, Params, initialize_model};
use ixa::{prelude::*, profiling::ProfilingContextExt, run_with_custom_args};

#[derive(Args, Debug)]
struct CustomArgs {
    #[arg(long)]
    synth_population: Option<PathBuf>,
}

fn main() {
    let mut context =
        run_with_custom_args(|context: &mut Context, args, custom: Option<CustomArgs>| {
            assert!(
                args.config.is_some(),
                "No config file provided, must follow `cargo run -- --config path/to/filename.json`"
            );

            let synth_population_override = custom.and_then(|c| c.synth_population);

            let &Params { seed, max_time, .. } = context.get_params();
            initialize_model(context, seed, max_time, synth_population_override)
                .expect("Model initialization failed");

            Ok(())
        })
        .unwrap();

    // Write the profiling data and context's execution statistics to a JSON file.
    context.write_profiling_data();
    ixa::profiling::print_profiling_data();
}
