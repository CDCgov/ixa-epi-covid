use std::path::{Path, PathBuf};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use epimodel::parameters::{GlobalParams, Params, RateFnType};
use epimodel::settings::{SettingCategory, SettingProperties};
use epimodel::symptom_status_manager::SymptomDelayDistLogNormParams;
use epimodel::{ContextParametersExt, initialize_model};
use ixa::HashMap;
use ixa::prelude::*;

fn make_params(synth_file: PathBuf) -> Params {
    let delay = SymptomDelayDistLogNormParams {
        mu: 0.1,
        sigma: 0.1,
    };
    Params {
        seed: 42,
        max_time: 10.0,
        initial_prevalence: 0.01,
        synth_population_file: synth_file,
        infectiousness_rate_fn: RateFnType::Constant {
            rate: 1.5,
            duration: 5.0,
        },
        probability_mild_given_infect: 0.7,
        infect_to_mild_delay: delay,
        probability_severe_given_mild: 0.2,
        mild_to_severe_delay: delay,
        mild_to_resolved_delay: delay,
        probability_critical_given_severe: 0.2,
        severe_to_critical_delay: delay,
        severe_to_resolved_delay: delay,
        probability_dead_given_critical: 0.2,
        critical_to_dead_delay: delay,
        critical_to_resolved_delay: delay,
        settings_properties: HashMap::from_iter([
            (SettingCategory::Home, SettingProperties { alpha: 0.3 }),
            (SettingCategory::Work, SettingProperties { alpha: 0.2 }),
            (SettingCategory::School, SettingProperties { alpha: 0.2 }),
            (
                SettingCategory::Community,
                SettingProperties { alpha: 0.01 },
            ),
        ]),
        itinerary_ratios: HashMap::from_iter([
            (SettingCategory::Home, 0.3),
            (SettingCategory::Work, 0.3),
            (SettingCategory::School, 0.1),
            (SettingCategory::Community, 0.3),
        ]),
        ..Default::default()
    }
}

fn setup_model(synth_file: &Path) -> Context {
    let mut context = Context::new();
    let params = make_params(synth_file.to_path_buf());
    context
        .set_global_property_value(GlobalParams, params)
        .unwrap();
    let &Params { seed, max_time, .. } = context.get_params();
    initialize_model(&mut context, seed, max_time, None).unwrap();
    context
}

const SIZES: &[(&str, &str)] = &[
    ("10k", "input/synth_pop_people_WY_10_000.csv"),
    ("100k", "input/synth_pop_people_WY_100_000.csv"),
];

fn bench_init(c: &mut Criterion) {
    let mut group = c.benchmark_group("init");
    group.sample_size(10);

    for &(label, path) in SIZES {
        let synth_file = PathBuf::from(path);
        if !synth_file.exists() {
            eprintln!("Skipping {label}: {path} not found");
            continue;
        }
        group.bench_with_input(
            BenchmarkId::new("population", label),
            &synth_file,
            |b, f| {
                b.iter(|| setup_model(f));
            },
        );
    }

    group.finish();
}

fn bench_execute(c: &mut Criterion) {
    let mut group = c.benchmark_group("execute");
    group.sample_size(10);

    for &(label, path) in SIZES {
        let synth_file = PathBuf::from(path);
        if !synth_file.exists() {
            eprintln!("Skipping {label}: {path} not found");
            continue;
        }
        group.bench_with_input(
            BenchmarkId::new("population", label),
            &synth_file,
            |b, f| {
                b.iter_batched(
                    || setup_model(f),
                    |mut context| context.execute(),
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_init, bench_execute);
criterion_main!(benches);
