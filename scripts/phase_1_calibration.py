from __future__ import annotations

import argparse
import json
import os
import pickle
import shutil
import tempfile
import timeit
import warnings
from copy import deepcopy
from pathlib import Path
from typing import Any

import polars as pl
from calibrationtools import (
    ABCSampler,
    AdaptMultivariateNormalVariance,
    CSVTableOutputContract,
    IndependentKernels,
    MRPOutputRunner,
    MultivariateNormalKernel,
    Particle,
    ParticleReader,
    SeedKernel,
)
from calibrationtools.cloud.auto_size import (
    CloudSizing,
    print_cloud_auto_size_summary,
    resolve_cloud_sizing_from_config,
)
from calibrationtools.cloud.runner import create_cloud_mrp_runner_from_config
from create_synthetic_population.run import run as create_synthetic_population
from dotenv import load_dotenv
from requests.exceptions import HTTPError
from us import states

from ixa_epi_covid import (
    CovidModel,
    CovidModelConfig,
    update_epimodel_output_dir,
)
from ixa_epi_covid.covid_model import (
    CANONICAL_OUTPUT_FILENAME,
    PHASE1_OUTPUT_NAME,
)

TARGET_DATA = pl.DataFrame({"t": [75], "count": [1]})
DEFAULT_MAX_CONCURRENT_SIMULATIONS = 10
DEFAULT_ARTIFACTS_DIR = Path("experiments/phase1/calibration/artifacts")
DEFAULT_CLOUD_CONFIG_PATH = Path("ixa_epi_covid.cloud_config.toml")
DEFAULT_DOCKER_MRP_CONFIG_PATH = Path("ixa_epi_covid.mrp.docker.toml")
DOCKER_IXA_EXECUTABLE = "/app/target/release/ixa-epi-covid"
PHASE1_ENTROPY = 0x2D845A9183A835EC4A777F6C7403A6D0


def get_synth_pop_file(
    config: CovidModelConfig,
    default_population_size_dev: str,
) -> str:
    """Resolve the synthetic population file, creating a dev file if needed."""
    load_dotenv()
    synth_pop_file_env = os.getenv("SYNTH_POP_FILE")

    if synth_pop_file_env and config.use_env_synth_pop_file:
        print(
            "Using the synth population file specified in environment "
            f"variable SYNTH_POP_FILE: {synth_pop_file_env}"
        )
        if os.path.exists(synth_pop_file_env):
            filename = os.path.basename(synth_pop_file_env)
            local_synth_pop_file = Path(
                "experiments",
                "phase1",
                "input",
                filename,
            )
            if not local_synth_pop_file.exists() and synth_pop_file_env != str(
                local_synth_pop_file
            ):
                local_synth_pop_file.parent.mkdir(
                    parents=True,
                    exist_ok=True,
                )
                shutil.copyfile(synth_pop_file_env, local_synth_pop_file)
        else:
            raise FileNotFoundError(
                "Synth population file specified in environment variable "
                f"SYNTH_POP_FILE not found at path: {synth_pop_file_env}"
            )
        return str(local_synth_pop_file)

    us_state = states.lookup(config.state)
    if us_state is None:
        raise ValueError(f"Unknown state in phase-1 config: {config.state}")
    state_abbr = us_state.abbr
    input_file = Path(
        "input",
        f"synth_pop_people_{state_abbr}_{default_population_size_dev}.csv",
    )

    print(
        f"Creating a default synth population file for {us_state.name}: "
        f"{input_file}."
    )
    if not input_file.exists():
        try:
            create_synthetic_population(
                [
                    "--size",
                    default_population_size_dev,
                    "--state",
                    state_abbr,
                    "--year",
                    str(config.year),
                ]
            )
        except HTTPError:
            warnings.warn(
                "Failed to create synthetic population file for "
                f"{us_state.name} using the year {config.year}. Trying again "
                "with the default year 2023.",
                stacklevel=2,
            )
            create_synthetic_population(
                ["--size", default_population_size_dev, "--state", state_abbr]
            )
    return str(input_file)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse phase-1 calibration CLI arguments."""
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
        help="Run simulations through the given local MRP config.",
    )
    parser.add_argument(
        "--cloud-config",
        type=Path,
        default=DEFAULT_CLOUD_CONFIG_PATH,
        help="Cloud config used by --cloud and --auto-size.",
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
        default="50_000",
        help=(
            "Synthetic population size to generate in development when "
            "SYNTH_POP_FILE is not provided."
        ),
    )
    parser.add_argument(
        "--artifacts-dir",
        type=Path,
        default=None,
        help="Root directory where calibration stages particle input/output.",
    )
    parser.add_argument(
        "--no-artifacts",
        action="store_true",
        help="Disable local input/output artifact staging. Invalid with cloud.",
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
    return parser.parse_args(argv)


def resolve_max_concurrent_simulations(args: argparse.Namespace) -> int:
    """Return the selected simulation concurrency from new or legacy flags."""
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
    """Return whether the user explicitly selected calibration concurrency."""
    return (
        args.max_concurrent_simulations is not None
        or args.max_workers is not None
    )


def resolve_artifacts_dir(args: argparse.Namespace) -> Path | None:
    """Return the artifact staging directory for the selected runner mode."""
    if args.no_artifacts:
        if args.artifacts_dir is not None:
            raise ValueError(
                "Pass either --artifacts-dir or --no-artifacts, not both."
            )
        if args.cloud:
            raise ValueError(
                "--cloud requires artifacts; omit --no-artifacts or pass "
                "--artifacts-dir."
            )
        return None

    if args.artifacts_dir is not None:
        return args.artifacts_dir
    if args.cloud or args.docker or args.mrp_config is not None:
        return DEFAULT_ARTIFACTS_DIR
    return None


def build_phase1_output_contract() -> CSVTableOutputContract:
    """Build the CSV-table contract used by MRP, Docker, and cloud runners."""
    return CSVTableOutputContract(
        filename=CANONICAL_OUTPUT_FILENAME,
        output_name=PHASE1_OUTPUT_NAME,
        orientation="columns",
    )


def apply_local_docker_runtime_overrides(
    model_inputs: dict[str, Any],
    *,
    docker: bool,
    cloud: bool,
    mrp_config: str | Path | None,
) -> dict[str, Any]:
    """Return inputs with the in-container executable path for Docker mode."""
    if not docker or cloud or mrp_config is not None:
        return model_inputs

    resolved_inputs = deepcopy(model_inputs)
    config_inputs = resolved_inputs.get("config_inputs")
    if not isinstance(config_inputs, dict):
        raise ValueError(
            "Docker phase-1 calibration inputs must include config_inputs."
        )
    config_inputs["exe_file"] = DOCKER_IXA_EXECUTABLE
    return resolved_inputs


def resolve_cloud_sizing(
    args: argparse.Namespace,
    *,
    base_inputs: dict[str, Any],
) -> CloudSizing:
    """Resolve optional cloud auto-size settings from the project TOML."""
    return resolve_cloud_sizing_from_config(
        cloud_config_path=args.cloud_config,
        base_inputs=base_inputs,
        auto_size=args.auto_size,
        cloud=args.cloud,
        max_concurrent_simulations=resolve_max_concurrent_simulations(args),
        max_concurrent_simulations_explicit=(
            _max_concurrent_simulations_was_explicit(args)
        ),
    )


def resolve_model_runner(
    args: argparse.Namespace,
    base_inputs: dict[str, Any],
    generation_count: int,
    cloud_sizing: CloudSizing | None,
):
    """Create the direct, MRP, Docker, or cloud model runner."""
    output_contract = build_phase1_output_contract()
    if args.cloud:
        if cloud_sizing is None:
            cloud_sizing = CloudSizing(
                max_concurrent_simulations=(
                    resolve_max_concurrent_simulations(args)
                )
            )
        return create_cloud_mrp_runner_from_config(
            args.cloud_config,
            generation_count=generation_count,
            max_concurrent_simulations=(
                cloud_sizing.max_concurrent_simulations
            ),
            output_contract=output_contract,
            base_inputs=base_inputs,
            print_task_durations=args.print_task_durations,
            task_slots_per_node_override=(
                cloud_sizing.task_slots_per_node_override
            ),
            auto_size_summary=cloud_sizing.summary,
        )
    if args.mrp_config is not None:
        return MRPOutputRunner(
            args.mrp_config, output_contract=output_contract
        )
    if args.docker:
        return MRPOutputRunner(
            DEFAULT_DOCKER_MRP_CONFIG_PATH,
            output_contract=output_contract,
        )
    return CovidModel()


def normalize_phase1_report(
    report: pl.DataFrame | dict[str, list[Any]],
) -> pl.DataFrame:
    """Normalize direct Polars and CSV-table outputs to one DataFrame shape."""
    frame = (
        report if isinstance(report, pl.DataFrame) else pl.DataFrame(report)
    )
    casts = []
    if "t_lower" in frame.columns:
        casts.append(pl.col("t_lower").cast(pl.Float64))
    if "t_upper" in frame.columns:
        casts.append(pl.col("t_upper").cast(pl.Float64))
    if "count" in frame.columns:
        casts.append(pl.col("count").cast(pl.Int64))
    if not casts:
        return frame
    try:
        return frame.with_columns(casts)
    except Exception as exc:
        raise ValueError(
            "Phase-1 report columns t_lower, t_upper, and count must be "
            "numeric or numeric strings."
        ) from exc


def outputs_to_distance(
    model_output: dict[str, Any],
    target_data: pl.DataFrame,
) -> float:
    """Score model outputs against the observed first-death target."""
    report = normalize_phase1_report(model_output[PHASE1_OUTPUT_NAME])
    first_death_observed = report.filter(pl.col("count") > 0).filter(
        pl.col("t_upper") == pl.min("t_upper")
    )
    if first_death_observed.height > 0:
        return abs(
            target_data["t"][0] - first_death_observed.item(0, "t_upper")
        ) + (first_death_observed.height - target_data["count"][0])
    return 1000.0


def main(
    config_file: str | Path,
    output_dir: str | Path,
    max_workers: int | None = None,
    default_population_size_dev: str = "50_000",
    *,
    max_concurrent_simulations: int | None = None,
    docker: bool = False,
    cloud: bool = False,
    mrp_config: str | Path | None = None,
    cloud_config: str | Path = DEFAULT_CLOUD_CONFIG_PATH,
    auto_size: bool = False,
    artifacts_dir: str | Path | None = None,
    no_artifacts: bool = False,
    print_task_durations: bool = False,
    print_task_progress: bool = False,
):
    """Run phase-1 calibration in direct, MRP, Docker, or cloud mode."""
    args = argparse.Namespace(
        config_file=Path(config_file),
        output_dir=Path(output_dir),
        max_workers=max_workers,
        max_concurrent_simulations=max_concurrent_simulations,
        default_population_size_dev=default_population_size_dev,
        docker=docker,
        cloud=cloud,
        mrp_config=Path(mrp_config) if mrp_config is not None else None,
        cloud_config=Path(cloud_config),
        auto_size=auto_size,
        artifacts_dir=Path(artifacts_dir)
        if artifacts_dir is not None
        else None,
        no_artifacts=no_artifacts,
        print_task_durations=print_task_durations,
        print_task_progress=print_task_progress,
    )

    if args.auto_size and not args.cloud:
        raise ValueError("--auto-size requires --cloud")

    resolved_artifacts_dir = resolve_artifacts_dir(args)

    # Load environment files, defaults, and setup configurations.
    config = CovidModelConfig(
        config_file=args.config_file,
        target_data=TARGET_DATA,
    )

    # Update IXA overrides.
    ixa_overrides = {
        "max_time": config.target_data["t"][0] + config.tolerance_values[0] + 1
    }

    synth_pop_file = get_synth_pop_file(
        config,
        default_population_size_dev=args.default_population_size_dev,
    )
    if args.cloud:
        print(
            "Cloud calibration will stage synthetic population as a shared "
            f"asset: {synth_pop_file}"
        )
    ixa_overrides.update({"synth_population_file": synth_pop_file})
    config.update_ixa_params(ixa_overrides)

    # Generate MRP defaults.
    mrp_defaults = config.get_mrp_defaults_for_output(
        args.output_dir,
        outputs_to_read=[PHASE1_OUTPUT_NAME],
    )
    mrp_defaults = apply_local_docker_runtime_overrides(
        mrp_defaults,
        docker=args.docker,
        cloud=args.cloud,
        mrp_config=args.mrp_config,
    )

    # Make the output directory.
    if os.path.exists(args.output_dir):
        if config.force_overwrite:
            shutil.rmtree(str(args.output_dir))
        else:
            raise FileExistsError(
                f"Output directory {args.output_dir} already exists and "
                "force_overwrite is set to False."
            )

    Path(args.output_dir).mkdir(parents=True, exist_ok=False)

    # Create the priors and perturbation kernels.
    with open(config.priors_file) as f:
        priors = json.load(f)

    particle_param_names = list(priors["priors"].keys()) + ["seed"]
    reader = ParticleReader(
        particle_param_names=particle_param_names,
        default_params=mrp_defaults,
    )

    def particles_to_params(particle: Particle) -> dict[str, Any]:
        """Convert a particle into model params with a unique output dir."""
        particle_params = reader.read_particle(particle=particle)
        simulations_dir = Path(
            particle_params["config_inputs"]["output_dir"],
            "simulations",
        )
        simulations_dir.mkdir(parents=True, exist_ok=True)
        seed = int(
            particle_params["ixa_inputs"]["epimodel.GlobalParams"]["seed"]
        )
        output_path = Path(
            tempfile.mkdtemp(prefix=f"{seed}.", dir=simulations_dir)
        )
        return update_epimodel_output_dir(particle_params, output_path)

    if args.cloud:
        cloud_sizing = resolve_cloud_sizing(args, base_inputs=mrp_defaults)
        print_cloud_auto_size_summary(cloud_sizing)
        max_simulations = cloud_sizing.max_concurrent_simulations
    else:
        cloud_sizing = None
        max_simulations = resolve_max_concurrent_simulations(args)

    model_runner = resolve_model_runner(
        args,
        base_inputs=mrp_defaults,
        generation_count=len(config.tolerance_values),
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
            max_concurrent_simulations=max_simulations,
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

        with open(Path(args.output_dir, "results.pkl"), "wb") as fp:
            pickle.dump(results, fp)
        with open(Path(args.output_dir, "config.pkl"), "wb") as fp:
            pickle.dump(config, fp)
        return results
    finally:
        close = getattr(model_runner, "close", None)
        if callable(close):
            close()


def cli(argv: list[str] | None = None) -> int:
    """Run phase-1 calibration from command-line arguments."""
    args = parse_args(argv)
    main(
        config_file=args.config_file,
        output_dir=args.output_dir,
        max_workers=args.max_workers,
        max_concurrent_simulations=args.max_concurrent_simulations,
        default_population_size_dev=args.default_population_size_dev,
        docker=args.docker,
        cloud=args.cloud,
        mrp_config=args.mrp_config,
        cloud_config=args.cloud_config,
        auto_size=args.auto_size,
        artifacts_dir=args.artifacts_dir,
        no_artifacts=args.no_artifacts,
        print_task_durations=args.print_task_durations,
        print_task_progress=args.print_task_progress,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(cli())
