from __future__ import annotations

import json
import os
import pickle
import re
import shutil
import tempfile
import warnings
from pathlib import Path
from typing import Any, Callable

import polars as pl
from calibrationtools import Particle
from create_synthetic_population.run import (
    run as create_synthetic_population_run,
)
from dotenv import load_dotenv
from particle_reader import ParticleReader
from requests.exceptions import HTTPError
from us import states

from ixa_epi_covid import CovidModelConfig, update_epimodel_output_dir
from ixa_epi_covid.model_execution import (
    PHASE1_OUTPUT_NAME,
    phase1_rows_to_report,
)

DEFAULT_TARGET_DATA = pl.DataFrame(
    {
        "t": [75],
        "count": [1],
    }
)
DEFAULT_DEV_POPULATION_SIZE = "50_000"
_SYNTH_POPULATION_FILENAME_RE = re.compile(
    r"^synth_pop_people_[A-Z]{2}_(?P<size>[0-9][0-9_]*)\.csv$"
)


def load_phase1_config(
    config_file: str | Path,
    *,
    target_data: pl.DataFrame | None = None,
) -> CovidModelConfig:
    return CovidModelConfig(
        config_file=config_file,
        target_data=target_data
        if target_data is not None
        else DEFAULT_TARGET_DATA,
    )


def resolve_synth_population_file(
    config: CovidModelConfig,
    *,
    default_population_size_dev: str = DEFAULT_DEV_POPULATION_SIZE,
    env: dict[str, str] | None = None,
    create_population_func: Callable[[list[str]], Any] | None = None,
) -> Path:
    """Resolve the synth-population CSV needed by phase-1 calibration."""
    load_dotenv()
    env = os.environ if env is None else env
    if create_population_func is None:
        create_population_func = create_synthetic_population_run

    synth_pop_file_env = env.get("SYNTH_POP_FILE")
    if synth_pop_file_env and config.use_env_synth_pop_file:
        env_path = Path(synth_pop_file_env)
        if not env_path.exists():
            raise FileNotFoundError(
                "Synth population file specified in environment variable "
                f"SYNTH_POP_FILE not found at path: {env_path}"
            )
        local_synth_pop_file = Path(
            "experiments",
            "phase1",
            "input",
            env_path.name,
        )
        if env_path.resolve() != local_synth_pop_file.resolve():
            local_synth_pop_file.parent.mkdir(parents=True, exist_ok=True)
            if not local_synth_pop_file.exists():
                shutil.copyfile(env_path, local_synth_pop_file)
        return local_synth_pop_file

    us_state = states.lookup(config.state)
    if us_state is None:
        raise ValueError(f"Could not resolve US state from {config.state!r}")

    input_file = Path(
        "input",
        f"synth_pop_people_{us_state.abbr}_{default_population_size_dev}.csv",
    )
    if input_file.exists():
        return input_file

    create_args = [
        "--size",
        default_population_size_dev,
        "--state",
        us_state.abbr,
        "--year",
        str(config.year),
    ]
    try:
        create_population_func(create_args)
    except HTTPError:
        warnings.warn(
            "Failed to create synthetic population file for "
            f"{us_state.name} using the year {config.year}. "
            "Trying again with the default year 2023.",
            stacklevel=2,
        )
        create_population_func(
            [
                "--size",
                default_population_size_dev,
                "--state",
                us_state.abbr,
            ]
        )
    return input_file


def infer_synth_population_size_label(
    synth_population_file: str | Path,
) -> str | None:
    """Infer the configured synthetic population size from its filename."""
    match = _SYNTH_POPULATION_FILENAME_RE.match(
        Path(synth_population_file).name
    )
    if match is None:
        return None
    return match.group("size")


def format_synth_population_summary(
    synth_population_file: str | Path,
    *,
    cloud: bool,
) -> str:
    """Summarize which synth-population asset calibration will use."""
    synth_population_path = Path(synth_population_file)
    size_label = infer_synth_population_size_label(synth_population_path)
    size_text = (
        f"population size {size_label}"
        if size_label is not None
        else "population size unknown"
    )
    usage_text = (
        "staged once and shared by all cloud simulations"
        if cloud
        else "shared by all simulations"
    )
    return (
        f"Using synthetic population file {synth_population_path} "
        f"({size_text}, {usage_text})."
    )


def build_runtime_ixa_overrides(
    config: CovidModelConfig,
    *,
    synth_population_file: str | Path,
) -> dict[str, Any]:
    return {
        "max_time": config.target_data["t"][0]
        + config.tolerance_values[0]
        + 1,
        "synth_population_file": str(synth_population_file),
    }


def prepare_output_dir(
    output_dir: str | Path,
    *,
    force_overwrite: bool,
) -> Path:
    output_dir = Path(output_dir)
    if output_dir.exists():
        if force_overwrite:
            shutil.rmtree(output_dir)
        else:
            raise FileExistsError(
                f"Output directory {output_dir} already exists and "
                "force_overwrite is set to False."
            )
    output_dir.mkdir(parents=True, exist_ok=False)
    return output_dir


def load_priors(priors_file: str | Path) -> dict[str, Any]:
    with Path(priors_file).open(encoding="utf-8") as f:
        return json.load(f)


def build_particles_to_params(
    *,
    default_params: dict[str, Any],
    particle_param_names: list[str],
) -> Callable[[Particle], dict[str, Any]]:
    reader = ParticleReader(
        particle_param_names=particle_param_names,
        default_params=default_params,
    )

    def particles_to_params(
        particle: Particle,
        reader: ParticleReader = reader,
    ) -> dict[str, Any]:
        particle_params = reader.read_particle(particle=particle)
        simulations_dir = Path(
            particle_params["config_inputs"]["output_dir"],
            "simulations",
        )
        simulations_dir.mkdir(parents=True, exist_ok=True)
        seed = particle_params["ixa_inputs"]["epimodel.GlobalParams"]["seed"]
        particle_output_dir = Path(
            tempfile.mkdtemp(
                prefix=f"{seed}.",
                dir=simulations_dir,
            )
        )
        return update_epimodel_output_dir(
            particle_params,
            particle_output_dir,
        )

    return particles_to_params


def outputs_to_distance(
    model_output: dict[str, Any],
    target_data: pl.DataFrame,
) -> float:
    """Score the first-death timing error plus overcount penalty."""
    report = phase1_rows_to_report(model_output[PHASE1_OUTPUT_NAME])
    first_death_observed = report.filter(pl.col("count") > 0).filter(
        pl.col("t_upper") == pl.min("t_upper")
    )
    if first_death_observed.height > 0:
        return abs(
            target_data["t"][0] - first_death_observed.item(0, "t_upper")
        ) + (first_death_observed.height - target_data["count"][0])
    return 1000.0


def save_calibration_artifacts(
    *,
    output_dir: str | Path,
    results: Any,
    config: CovidModelConfig,
) -> None:
    output_dir = Path(output_dir)
    with (output_dir / "results.pkl").open("wb") as fp:
        pickle.dump(results, fp)
    with (output_dir / "config.pkl").open("wb") as fp:
        pickle.dump(config, fp)
