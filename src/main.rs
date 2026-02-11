use epimodel::{ContextParametersExt, Params, initialize_model};
use ixa::{Context, profiling::ProfilingContextExt, run_with_args};

fn main() {
    let mut context = run_with_args(|context: &mut Context, args, _| {
        assert!(
            args.config.is_some(),
            "No config file provided, must follow `cargo run -- --config path/to/filename.json`"
        );

        // Assert that the binary exists
        assert!(
            std::path::Path::new("target/release/ixa-epi-covid").exists(),
            "Binary target/release/ixa-epi-covid does not exist"
        );

        let &Params { seed, max_time, .. } = context.get_params();
        initialize_model(context, seed, max_time).expect("Model initialization failed");

        Ok(())
    })
    .unwrap();

    // Write the profiling data and context's execution statistics to a JSON file.
    context.write_profiling_data();
    ixa::profiling::print_profiling_data();
}
