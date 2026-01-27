use epimodel::{ParametersContextExt, Params, run_model};
use ixa::{Context, profiling::ProfilingContextExt, run_with_args};

fn main() {
    let mut context = run_with_args(|context: &mut Context, args, _| {
        assert!(
            args.config.is_some(),
            "No config file provided, must follow `cargo run -- --config path/to/filename.json`"
        );

        let &Params { seed, max_time, .. } = context.get_params();
        run_model(context, seed, max_time).expect("Model run failed");

        Ok(())
    })
    .unwrap();

    // Write the profiling data and context's execution statistics to a JSON file.
    context.write_profiling_data();
    ixa::profiling::print_profiling_data();
}
