from __future__ import annotations

import argparse
import json
import sys
import timeit
import warnings
from pathlib import Path
from typing import Any

from calibrationtools import (
    ABCSampler,
    AdaptMultivariateNormalVariance,
    IndependentKernels,
    MultivariateNormalKernel,
    SeedKernel,
)
from calibrationtools.cloud.auto_size import (
    CloudSizing,
    resolve_cloud_auto_size,
    run_local_memory_probe,
)

from ixa_epi_covid import IxaEpiCovidDirectRunner
from ixa_epi_covid.cloud.runner import (
    IxaEpiCovidCloudRunner,
    resolve_cloud_build_context,
)
from ixa_epi_covid.cloud.utils import load_cloud_runtime_settings
from ixa_epi_covid.model_execution import PHASE1_OUTPUT_NAME
from ixa_epi_covid.mrp_runner import (
    DEFAULT_CLOUD_MRP_CONFIG_PATH,
    DEFAULT_DOCKER_MRP_CONFIG_PATH,
    IxaEpiCovidMRPRunner,
)

from .core import (
    DEFAULT_DEV_POPULATION_SIZE,
    DEFAULT_TARGET_DATA,
    build_particles_to_params,
    build_runtime_ixa_overrides,
    format_synth_population_summary,
    load_phase1_config,
    load_priors,
    outputs_to_distance,
    prepare_output_dir,
    resolve_synth_population_file,
    save_calibration_artifacts,
)

DEFAULT_MAX_CONCURRENT_SIMULATIONS = 10
DEFAULT_ARTIFACTS_DIR = Path("artifacts")
PHASE1_ENTROPY = 0x2D845A9183A835EC4A777F6C7403A6D0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run phase 1 calibration for ixa-epi-covid."
    )
    mode_group = parser.add_mutually_exclusive_group()
    mode_group.add_argument(
        "--docker",
        action="store_true",
        help="Run simulations through the Docker-backed MRP config.",
    )
    mode_group.add_argument(
        "--cloud",
        action="store_true",
        help="Run simulations through the cloud-backed MRP config.",
    )
    mode_group.add_argument(
        "--mrp-config",
        type=Path,
        help="Run simulations through the given MRP config path.",
    )
    parser.add_argument(
        "--config_file",
        "-c",
        type=Path,
        required=True,
        help="Path to the phase-1 calibration configuration file.",
    )
    parser.add_argument(
        "--output-dir",
        "-o",
        type=Path,
        default=Path("experiments/phase1/calibration/output"),
        help="Path to the output directory where results will be saved.",
    )
    parser.add_argument(
        "--max-workers",
        type=int,
        default=None,
        help="Backward-compatible alias for --max-concurrent-simulations.",
    )
    parser.add_argument(
        "--max-concurrent-simulations",
        type=int,
        default=None,
        help="Maximum number of simulations to evaluate at once.",
    )
    parser.add_argument(
        "--auto-size",
        action="store_true",
        help=(
            "Cloud mode only. Run one local probe simulation before Azure "
            "provisioning and set task slots from measured RAM usage."
        ),
    )
    parser.add_argument(
        "--default-population-size-dev",
        type=str,
        default=DEFAULT_DEV_POPULATION_SIZE,
        help=(
            "Synthetic population size to generate in development when "
            "SYNTH_POP_FILE is not provided."
        ),
    )
    parser.add_argument(
        "--print-task-durations",
        action="store_true",
        help="Print per-task timing information in cloud mode.",
    )
    parser.add_argument(
        "--print-task-progress",
        action="store_true",
        help="Print generation-level calibration progress.",
    )
    parser.add_argument(
        "--artifacts-dir",
        type=Path,
        default=None,
        help=(
            "Root directory where calibration stages particle input/output. "
            f"Defaults to {DEFAULT_ARTIFACTS_DIR}."
        ),
    )
    parser.add_argument(
        "--no-artifacts",
        action="store_true",
        help=(
            "Disable local input/output artifact staging. Not valid with --cloud."
        ),
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=None,
        help="Docker build context root for cloud mode.",
    )
    parser.add_argument(
        "--dockerfile",
        type=Path,
        default=None,
        help="Dockerfile path for cloud mode.",
    )
    return parser.parse_args(argv)


def resolve_max_concurrent_simulations(args: argparse.Namespace) -> int:
    value = (
        args.max_concurrent_simulations
        if args.max_concurrent_simulations is not None
        else args.max_workers
    )
    if value is None:
        value = DEFAULT_MAX_CONCURRENT_SIMULATIONS
    if value < 1:
        raise ValueError(
            f"--max-concurrent-simulations must be at least 1 (got {value})"
        )
    return value


def _max_concurrent_simulations_was_explicit(
    args: argparse.Namespace,
) -> bool:
    return (
        args.max_concurrent_simulations is not None
        or args.max_workers is not None
    )


def resolve_artifacts_dir(args: argparse.Namespace) -> Path | None:
    artifacts_dir = args.artifacts_dir
    if args.no_artifacts:
        if artifacts_dir is not None:
            raise ValueError(
                "Pass either --artifacts-dir or --no-artifacts, not both."
            )
        if args.cloud:
            raise ValueError(
                "--cloud requires artifacts; omit --no-artifacts or pass "
                "--artifacts-dir."
            )
        return None
    return (
        artifacts_dir if artifacts_dir is not None else DEFAULT_ARTIFACTS_DIR
    )


def resolve_cloud_sizing(
    args: argparse.Namespace,
    *,
    base_inputs: dict[str, Any],
) -> CloudSizing:
    max_concurrent_simulations = resolve_max_concurrent_simulations(args)
    if not args.auto_size or not args.cloud:
        return resolve_cloud_auto_size(
            auto_size=args.auto_size,
            cloud=args.cloud,
            max_concurrent_simulations=max_concurrent_simulations,
            max_concurrent_simulations_explicit=(
                _max_concurrent_simulations_was_explicit(args)
            ),
        )

    settings = load_cloud_runtime_settings(DEFAULT_CLOUD_MRP_CONFIG_PATH)
    return resolve_cloud_auto_size(
        auto_size=args.auto_size,
        cloud=args.cloud,
        max_concurrent_simulations=max_concurrent_simulations,
        max_concurrent_simulations_explicit=(
            _max_concurrent_simulations_was_explicit(args)
        ),
        vm_size=settings.vm_size,
        pool_max_nodes=settings.pool_max_nodes,
        measure_task_peak_rss_bytes=(
            lambda: run_local_memory_probe(
                "ixa_epi_covid.cloud.auto_size",
                base_inputs,
            )
        ),
    )


def _format_bytes(size: int) -> str:
    if size >= 1024**3:
        return f"{size / 1024**3:.1f} GiB"
    if size >= 1024**2:
        return f"{size / 1024**2:.1f} MiB"
    return f"{size} bytes"


def print_cloud_auto_size_summary(sizing: CloudSizing) -> None:
    summary = sizing.summary
    if summary is None:
        return

    cap_note = ""
    if summary.task_slots_per_node < summary.memory_task_slots_per_node:
        cap_note = (
            f", capped_from_ram_slots={summary.memory_task_slots_per_node}"
        )

    print(
        (
            "[cloud-run] auto-size simulation RAM "
            f"measured_peak_rss="
            f"{summary.measured_task_peak_rss_bytes} bytes "
            f"({_format_bytes(summary.measured_task_peak_rss_bytes)}), "
            f"vm_size={summary.vm_size}, "
            f"vm_ram={summary.vm_memory_bytes} bytes "
            f"({_format_bytes(summary.vm_memory_bytes)}), "
            f"reserve={summary.reserve:.0%}, "
            f"batch_slot_limit={summary.max_task_slots_per_node}, "
            f"task_slots_per_node={summary.task_slots_per_node}"
            f"{cap_note}, "
            f"max_concurrent_simulations_per_node="
            f"{summary.task_slots_per_node}, "
            f"max_concurrent_simulations_total="
            f"{sizing.max_concurrent_simulations}"
        ),
        file=sys.stderr,
        flush=True,
    )


def resolve_model_runner(
    args: argparse.Namespace,
    *,
    generation_count: int,
    synth_population_file: str | Path,
    cloud_sizing: CloudSizing | None = None,
):
    if args.cloud:
        if cloud_sizing is None:
            cloud_sizing = CloudSizing(
                max_concurrent_simulations=(
                    resolve_max_concurrent_simulations(args)
                )
            )
        repo_root, dockerfile = resolve_cloud_build_context(
            repo_root=args.repo_root,
            dockerfile=args.dockerfile,
        )
        return IxaEpiCovidCloudRunner(
            DEFAULT_CLOUD_MRP_CONFIG_PATH,
            generation_count=generation_count,
            max_concurrent_simulations=(
                cloud_sizing.max_concurrent_simulations
            ),
            synth_population_path=synth_population_file,
            repo_root=repo_root,
            dockerfile=dockerfile,
            print_task_durations=args.print_task_durations,
            task_slots_per_node_override=(
                cloud_sizing.task_slots_per_node_override
            ),
            auto_size_summary=cloud_sizing.summary,
        )
    if args.mrp_config is not None:
        return IxaEpiCovidMRPRunner(args.mrp_config)
    if args.docker:
        return IxaEpiCovidMRPRunner(DEFAULT_DOCKER_MRP_CONFIG_PATH)
    return IxaEpiCovidDirectRunner()


def run_phase1_calibration(
    *,
    config_file: str | Path,
    output_dir: str | Path,
    max_concurrent_simulations: int = DEFAULT_MAX_CONCURRENT_SIMULATIONS,
    default_population_size_dev: str = DEFAULT_DEV_POPULATION_SIZE,
    mrp_config: str | Path | None = None,
    docker: bool = False,
    cloud: bool = False,
    auto_size: bool = False,
    print_task_durations: bool = False,
    print_task_progress: bool = False,
    artifacts_dir: str | Path | None = None,
    no_artifacts: bool = False,
    repo_root: str | Path | None = None,
    dockerfile: str | Path | None = None,
):
    args = argparse.Namespace(
        config_file=Path(config_file),
        output_dir=Path(output_dir),
        max_workers=max_concurrent_simulations,
        max_concurrent_simulations=max_concurrent_simulations,
        default_population_size_dev=default_population_size_dev,
        mrp_config=Path(mrp_config) if mrp_config is not None else None,
        docker=docker,
        cloud=cloud,
        auto_size=auto_size,
        print_task_durations=print_task_durations,
        print_task_progress=print_task_progress,
        artifacts_dir=Path(artifacts_dir)
        if artifacts_dir is not None
        else None,
        no_artifacts=no_artifacts,
        repo_root=Path(repo_root) if repo_root is not None else None,
        dockerfile=Path(dockerfile) if dockerfile is not None else None,
    )
    return _run_calibration_from_args(args)


def _run_calibration_from_args(args: argparse.Namespace):
    if args.auto_size and not args.cloud:
        raise ValueError("--auto-size requires --cloud")
    resolved_artifacts_dir = resolve_artifacts_dir(args)

    config = load_phase1_config(
        args.config_file,
        target_data=DEFAULT_TARGET_DATA,
    )
    synth_population_file = resolve_synth_population_file(
        config,
        default_population_size_dev=args.default_population_size_dev,
    )
    print(
        format_synth_population_summary(
            synth_population_file,
            cloud=args.cloud,
        )
    )
    config.update_ixa_params(
        build_runtime_ixa_overrides(
            config,
            synth_population_file=synth_population_file,
        )
    )

    output_dir = prepare_output_dir(
        args.output_dir,
        force_overwrite=config.force_overwrite,
    )
    mrp_defaults = config.get_mrp_defaults_for_output(
        output_dir,
        outputs_to_read=[PHASE1_OUTPUT_NAME],
    )

    priors = load_priors(config.priors_file)
    particles_to_params = build_particles_to_params(
        default_params=mrp_defaults,
        particle_param_names=list(priors["priors"].keys()) + ["seed"],
    )

    cloud_sizing = resolve_cloud_sizing(args, base_inputs=mrp_defaults)
    print_cloud_auto_size_summary(cloud_sizing)

    model_runner = resolve_model_runner(
        args,
        generation_count=len(config.tolerance_values),
        synth_population_file=synth_population_file,
        cloud_sizing=cloud_sizing,
    )

    try:
        sampler = ABCSampler(
            generation_particle_count=config.generation_particle_count,
            tolerance_values=config.tolerance_values,
            priors=priors,
            perturbation_kernel=IndependentKernels(
                [
                    MultivariateNormalKernel(list(priors["priors"].keys())),
                    SeedKernel("seed"),
                ]
            ),
            particles_to_params=particles_to_params,
            variance_adapter=AdaptMultivariateNormalVariance(),
            outputs_to_distance=outputs_to_distance,
            target_data=config.target_data,
            model_runner=model_runner,
            max_concurrent_simulations=(
                cloud_sizing.max_concurrent_simulations
            ),
            entropy=PHASE1_ENTROPY,
            print_generation_progress=args.print_task_progress,
            artifacts_dir=resolved_artifacts_dir,
        )

        start = timeit.default_timer()
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", category=UserWarning)
            results = sampler.run()
        finish = timeit.default_timer()
        print(f"Calibration completed in {finish - start:.2f} seconds.")
        print(results)

        diagnostics = results.get_diagnostics()
        print("\nQuantiles for each parameter:")
        print(
            json.dumps(
                {
                    key: {k2: float(v2) for k2, v2 in value.items()}
                    for key, value in diagnostics["quantiles"].items()
                },
                indent=4,
            )
        )
        print("\nCorrelation matrix:")
        print(diagnostics["correlation_matrix"])

        save_calibration_artifacts(
            output_dir=output_dir,
            results=results,
            config=config,
        )
        return results
    finally:
        close = getattr(model_runner, "close", None)
        if callable(close):
            close()


def main(argv: list[str] | None = None) -> int:
    _run_calibration_from_args(parse_args(argv))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
