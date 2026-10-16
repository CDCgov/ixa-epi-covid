use std::path::PathBuf;

use clap::Args;
use epimodel::pop_reader::archive::set_data_path;
use epimodel::{ContextParametersExt, Params, initialize_model};
use ixa::{prelude::*, profiling::ProfilingContextExt, run_with_custom_args};

#[derive(Args, Debug, Default)]
struct CustomArgs {
    /// Synthetic population file, relative to the data path (overrides `synth_population_file`).
    #[arg(long)]
    synth_population: Option<PathBuf>,

    /// Directory (or zip archive) that population file paths are resolved against.
    /// Defaults to the current working directory.
    #[arg(long)]
    data_path: Option<PathBuf>,
}

fn main() {
    let mut context =
        run_with_custom_args(|context: &mut Context, args, custom: Option<CustomArgs>| {
            assert!(
                args.config.is_some(),
                "No config file provided, must follow `cargo run -- --config path/to/filename.json`"
            );

            let custom = custom.unwrap_or_default();
            let data_path = match custom.data_path {
                Some(path) => path,
                None => std::env::current_dir().expect("Could not determine working directory"),
            };
            set_data_path(data_path);
            let synth_population_override = custom.synth_population;
            println!("We are in main");
            let &Params { seed, max_time, .. } = context.get_params();
            initialize_model(context, seed, max_time, synth_population_override)
                .expect("Model initialization failed");

            Ok(())
        })
        .unwrap();

    // Write the profiling data and context's execution statistics to a JSON file.
    context.write_profiling_data();
}
