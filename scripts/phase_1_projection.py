import asyncio
from io import StringIO
import pickle
from pathlib import Path
from typing import Callable, Any, NoReturn
import threading

from calibrationtools import CalibrationResults, Particle
from concurrent.futures import ThreadPoolExecutor
from particle_reader import ParticleReader

from ixa_epi_covid import CovidModel, update_epimodel_output_dir
from ixa_epi_covid.config_parser import CovidModelConfig

import seaborn as sns
import matplotlib.pyplot as plt
import polars as pl
import os
import time
from rich.console import Console

# Run specific parameters declaration --------------------------------------------------------------------------------
rerun_particle_count = 2
max_workers = 4

# Load the calibration --------------------------------------------------------------------------------
calibration_output_dir = Path("experiments", "phase1", "calibration", "test")
with open(calibration_output_dir / "config.pkl", "rb") as f:
    config: CovidModelConfig = pickle.load(f)

with open(calibration_output_dir / "results.pkl", "rb") as f:
    results: CalibrationResults = pickle.load(f)

distances = results.flatten_distance_history()
for generation, errs in distances.items():
    print(f"Generation {generation} errors: {errs}")
    sns.histplot(errs, label=f"Generation {generation}")
    plt.show()

imported_cases_timeseries = []
aggregated_deaths_report = []
for particle in results.sample_posterior_particles(n=rerun_particle_count):
    seed = particle["seed"]
    simulation_output_dir = Path(
        calibration_output_dir,
        "simulations",
        str(seed),
    )
    imported_cases_timeseries.append(pl.read_csv(simulation_output_dir / "imported_cases_timeseries.csv").with_columns(pl.lit(seed).alias("seed")))
    aggregated_deaths_report.append(pl.read_csv(simulation_output_dir / "aggregated_deaths_report.csv").with_columns(pl.lit(seed).alias("seed")))

imported_cases_timeseries_df = pl.concat(imported_cases_timeseries)
aggregated_deaths_report_df = pl.concat(aggregated_deaths_report)

sns.lineplot(data=imported_cases_timeseries_df, x="time", y="imported_infections", units="seed", estimator=None)
plt.show()
sns.lineplot(data=aggregated_deaths_report_df.filter(pl.col("t_upper") > 70), x="t_upper", y="count", units="seed", estimator=None)
plt.show()

# Change parameters for projection --------------------------------------------------------------------------------
ixa_overrides = {
    "first_death_terminates_run": False,
    "prevalence_report": {"write": True},
    "max_time": 150.0,  # 3.5 months from the first reported death in Indiana
}

config.update_ixa_params(ixa_overrides)

mrp_defaults = config.get_mrp_defaults_for_output(
    output_dir=Path("experiments", "phase1", "projection", "output"),
    outputs_to_read=[
        "prevalence_report",
        "aggregated_deaths_report",
        "imported_cases_timeseries",
    ],
)

# Re-run particles with new parameters --------------------------------------------------------------------------------
particles = results.sample_posterior_particles(n=rerun_particle_count)
model = CovidModel()

# Model Particle Reader setup -------------------------------------------------------------

reader = ParticleReader(
    particle_param_names=results.fitted_params + ["seed"],
    default_params=mrp_defaults,
)


def particles_to_params(particle: Particle, reader: ParticleReader = reader):
    particle_params = reader.read_particle(particle=particle)
    # Make particle-specific output directory and update the output path in the parameters accordingly
    simulations_dir = Path(
        particle_params["config_inputs"]["output_dir"],
        "simulations"
    )
    # Count existing directories in simulations_dir
    if not simulations_dir.exists():
        dir_count = 0
    else:
        dir_count = len(os.walk(simulations_dir).__next__()[1]) 
    output_dir = Path(
        simulations_dir,
        ".".join([str(dir_count), str(particle_params["ixa_inputs"]["epimodel.GlobalParams"]["seed"])]),
    )
    output_dir.mkdir(parents=True, exist_ok=False)

    updated_params = update_epimodel_output_dir(particle_params, output_dir)
    return updated_params


def run_particle(particle: Particle):
    particle_params = particles_to_params(particle=particle)
    model.simulate(particle_params)

def get_console(verbose: bool = True) -> Console:
    """Return the console used for sampler reporting.

    This helper creates a visible Rich console for normal runs and a hidden
    in-memory console when sampler output should be suppressed.

    Args:
        verbose (bool): Whether console output should be visible.

    Returns:
        Console: Rich console configured for the requested verbosity.
    """

    if verbose:
        return Console(force_terminal=True)
    return Console(file=StringIO(), force_terminal=False)

class SamplerReporter:
    """Create progress displays and print run summaries.

    This helper owns the Rich console and the formatting of generation and run
    summaries so execution engines do not need to duplicate UI setup.

    Args:
        verbose (bool): Whether progress and summary output should be visible.
        console (Console | None): Optional console override used for tests.
    """

    def __init__(
        self,
        verbose: bool,
        console: Console | None = None,
    ) -> None:
        self.console = (
            console if console is not None else get_console(verbose)
        )
    

async def simulate_particles(
    particles: list[Particle],
    executor: ThreadPoolExecutor,
    chunksize: int = 1,
    reporter: SamplerReporter | None = None,
):
    if reporter is None:
        reporter = SamplerReporter()
    
    with reporter.create_weight_progress() as progress:
        assert executor is not None

        loop = asyncio.get_running_loop()
        worker = run_particle
        particle_chunks = [
            particles[index : index + chunksize]
            for index in range(0, len(particles), chunksize)
        ]
        tasks = []
        for chunk in particle_chunks:
            task = loop.run_in_executor(executor, worker, chunk)
            tasks.append((task, chunk))

        completed = 0
        try:
            for task, chunk in tasks:
                chunk_results = await task
                completed += 1
                progress.update(completed=completed)
        finally:
            for task, _ in tasks:
                task.cancel()

        reporter.print_timing_summary()

# Doesn't need to change across implementations, so we can keep it in calibrationtools to avoid code duplication
def run_coroutine_from_sync(coroutine_factory: Callable[[], Any]) -> Any:
    """Run an async workflow from synchronous code.

    This helper executes the coroutine directly when no event loop is active.
    If the caller already runs inside an event loop, it executes the coroutine
    in a dedicated worker thread and re-raises any exception from that thread.

    Args:
        coroutine_factory (Callable[[], Any]): Factory returning the coroutine
            to execute.

    Returns:
        Any: The value returned by the coroutine.
    """

    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(coroutine_factory())

    result: dict[str, Any] = {}
    error: dict[str, BaseException] = {}

    def runner() -> None:
        try:
            result["value"] = asyncio.run(coroutine_factory())
        except BaseException as exc:  # pragma: no cover - passthrough
            error["value"] = exc

    def raise_worker_error(exc: BaseException) -> NoReturn:
        raise exc

    thread = threading.Thread(target=runner, daemon=True)
    thread.start()
    thread.join()
    if "value" in error:
        raise_worker_error(error["value"])
    return result["value"]

start = time.time()

run_coroutine_from_sync(
    lambda: simulate_particles(
        executor=ThreadPoolExecutor(max_workers=max_workers),
        chunksize=1,
        reporter=SamplerReporter(),
        particles=particles,
    )
)


