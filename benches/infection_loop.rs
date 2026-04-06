use std::path::{Path, PathBuf};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
#[cfg(feature = "bench_old_settings")]
use epimodel::parameters::{CoreSettingsTypes, GlobalParams, Params, RateFnType};
#[cfg(not(feature = "bench_old_settings"))]
use epimodel::parameters::{GlobalParams, Params, RateFnType, SettingProperties};
#[cfg(not(feature = "bench_old_settings"))]
use epimodel::settings::SettingCategory;
#[cfg(feature = "bench_old_settings")]
use epimodel::settings::SettingProperties;
use epimodel::symptom_status_manager::SymptomDelayDistLogNormParams;
use epimodel::{ContextParametersExt, initialize_model};
use ixa::HashMap;
use ixa::prelude::*;

#[cfg(feature = "bench_old_settings")]
type SettingType = CoreSettingsTypes;
#[cfg(not(feature = "bench_old_settings"))]
type SettingType = SettingCategory;
fn make_params(synth_file: PathBuf) -> Params {
    let settings_file = synth_file.with_file_name(
        synth_file
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}_settings.csv", s))
            .unwrap_or_default(),
    );
    let delay = SymptomDelayDistLogNormParams {
        mu: 0.1,
        sigma: 0.1,
    };
    #[cfg(feature = "bench_old_settings")]
    let settings = [
        (SettingType::Home, 0.3, 0.3),
        (SettingType::Workplace, 0.2, 0.3),
        (SettingType::School, 0.2, 0.1),
        (SettingType::CensusTract, 0.01, 0.3),
    ];
    #[cfg(not(feature = "bench_old_settings"))]
    let settings = [
        (SettingType::Home, 0.3, 0.3),
        (SettingType::Work, 0.2, 0.3),
        (SettingType::School, 0.2, 0.1),
        (SettingType::Community, 0.01, 0.3),
    ];
    Params {
        seed: 42,
        max_time: 10.0,
        initial_prevalence: 0.01,
        synth_population_file: synth_file,
        settings_file: settings_file,
        infectiousness_rate_fn: RateFnType::Constant {
            rate: 1.5,
            duration: 5.0,
        },
        probability_mild_given_infect: 0.7,
        infect_to_mild_delay: delay,
        probability_severe_given_mild: HashMap::from_iter([("Age0To120".to_string(), 0.2)]),
        mild_to_severe_delay: delay,
        mild_to_resolved_delay: delay,
        probability_critical_given_severe: HashMap::from_iter([("Age0To120".to_string(), 0.2)]),
        severe_to_critical_delay: delay,
        severe_to_resolved_delay: delay,
        probability_dead_given_critical: HashMap::from_iter([("Age0To120".to_string(), 0.2)]),
        critical_to_dead_delay: delay,
        critical_to_resolved_delay: delay,
        settings_properties: HashMap::from_iter(
            settings
                .iter()
                .map(|(ty, alpha, _)| (*ty, SettingProperties { alpha: *alpha })),
        ),
        itinerary_ratios: HashMap::from_iter(settings.iter().map(|(ty, _, ratio)| (*ty, *ratio))),
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
    initialize_model(&mut context, seed, max_time, None, None).unwrap();
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
