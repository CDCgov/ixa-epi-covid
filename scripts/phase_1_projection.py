import argparse
import asyncio
import os
import pickle
import shutil
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any, Callable

import matplotlib.pyplot as plt
import seaborn as sns
from calibrationtools import (
    CalibrationResults,
    Particle,
    SamplerReporter,
    run_coroutine_from_sync,
)
from particle_reader import ParticleReader

from ixa_epi_covid import CovidModel, update_epimodel_output_dir
from ixa_epi_covid.config_parser import CovidModelConfig


async def run_particles(
    particles: list[Particle],
    executor: ThreadPoolExecutor,
    worker: Callable[[list[Particle]], Any],
    chunksize: int = 1,
    reporter: SamplerReporter | None = None,
):
    """
    Async function to call a worker funciton on each member of a list of Particles in parallel

    Args:
        particles (list[Particle]): List of particles to run the worker function on
        executor (ThreadPoolExecutor): Executor to run the worker function on
        worker (Callable[[list[Particle]], Any]): Worker function to run on each particle
        chunksize (int, optional): Number of particles to run in each chunk. Defaults to 1.
        reporter (SamplerReporter, optional): Reporter to use for progress reporting. Defaults to None, in which case a new SamplerReporter will be created.

    """
    if reporter is None:
        reporter = SamplerReporter()

    with reporter.create_task_progress() as progress:
        assert executor is not None
        start = time.time()
        handle = reporter.start_task(
            description="Simulating results from model... ",
            progress=progress,
            total=len(particles),
        )
        loop = asyncio.get_running_loop()
        particle_chunks = [
            particles[index : index + chunksize]
            for index in range(0, len(particles), chunksize)
        ]
        tasks = []
        for chunk in particle_chunks:
            task = loop.run_in_executor(executor, worker, chunk)
            tasks.append((task, chunk))

        try:
            for task, chunk in tasks:
                _chunk_results = await task
                reporter.advance(handle)
        finally:
            for task, _ in tasks:
                task.cancel()

        end = time.time()
        reporter.print_run_summary(end - start, process_name="Simulations")


def main(
    subdirectory_name: str,
    force_overwrite: bool = True,
    max_workers: int = 4,
    plot_distances: bool = True,
):
    # Run specific parameters declaration --------------------------------------------------------------------------------
    rerun_particle_count = 100
    if max_workers <= 1:
        parallel = False
    else:
        parallel = True

    # Load the calibration --------------------------------------------------------------------------------
    calibration_output_dir = Path(
        "experiments", "phase1", "calibration", subdirectory_name
    )
    if not calibration_output_dir.exists():
        raise FileNotFoundError(
            f"Calibration output directory {calibration_output_dir} does not exist. Run the calibration script before running this projection script."
        )
    projection_output_dir = Path(
        "experiments", "phase1", "projection", subdirectory_name
    )
    if projection_output_dir.exists():
        if force_overwrite:
            shutil.rmtree(projection_output_dir)
        else:
            raise FileExistsError(
                f"Projection output directory {projection_output_dir} already exists. Set force_overwrite to True to overwrite it."
            )

    with open(calibration_output_dir / "config.pkl", "rb") as f:
        config: CovidModelConfig = pickle.load(f)

    with open(calibration_output_dir / "results.pkl", "rb") as f:
        results: CalibrationResults = pickle.load(f)

    if plot_distances:
        distances = results.flatten_distance_history()
        for generation, errs in distances.items():
            sns.histplot(errs)
            plt.title(f"Distance distribution for generation {generation}")
            plt.xlabel("Distance")
            plt.ylabel("Count")
            plt.show()

    # Change parameters for projection --------------------------------------------------------------------------------
    ixa_overrides = {
        "first_death_terminates_run": False,
        "prevalence_report": {"write": True},
        "max_time": 150.0,  # 3.5 months from the first reported death in Indiana
    }

    config.update_ixa_params(ixa_overrides)

    mrp_defaults = config.get_mrp_defaults_for_output(
        output_dir=projection_output_dir,
        outputs_to_read=[],  # We do not need to read the outputs from the projection runs, so we can leave this empty
    )

    # Re-run particles with new parameters --------------------------------------------------------------------------------
    particles = results.sample_posterior_particles(n=rerun_particle_count)
    model = CovidModel()

    # Model Particle Reader setup -------------------------------------------------------------

    reader = ParticleReader(
        particle_param_names=results.fitted_params + ["seed"],
        default_params=mrp_defaults,
    )

    def particles_to_params(
        particle: Particle, reader: ParticleReader = reader
    ):
        particle_params = reader.read_particle(particle=particle)
        # Make particle-specific output directory and update the output path in the parameters accordingly
        simulations_dir = Path(
            particle_params["config_inputs"]["output_dir"], "simulations"
        )
        # Count existing directories in simulations_dir
        if not simulations_dir.exists():
            dir_count = 0
        else:
            dir_count = len(os.walk(simulations_dir).__next__()[1])
        output_dir = Path(
            simulations_dir,
            ".".join(
                [
                    str(dir_count),
                    str(
                        particle_params["ixa_inputs"]["epimodel.GlobalParams"][
                            "seed"
                        ]
                    ),
                ]
            ),
        )
        output_dir.mkdir(parents=True, exist_ok=False)

        updated_params = update_epimodel_output_dir(
            particle_params, output_dir
        )
        return updated_params

    def run_particle(particle: Particle):
        particle_params = particles_to_params(particle=particle)
        model.simulate(particle_params)

    def _evaluate_particle_chunk(
        particles: list[Particle],
    ) -> list[Any]:
        """Evaluate a chunk of proposed particles serially.

        This helper keeps chunk evaluation reusable between the serial and
        threaded batch-processing paths.

        Args:
            proposed_particles (list[Particle]): Proposed particles to score.
            particle_kwargs (dict[str, Any]): Additional keyword arguments
                forwarded into particle evaluation.
        """

        return [run_particle(particle=particle) for particle in particles]

    if parallel:
        run_coroutine_from_sync(
            lambda: run_particles(
                executor=ThreadPoolExecutor(max_workers=max_workers),
                worker=_evaluate_particle_chunk,
                chunksize=1,
                reporter=SamplerReporter(verbose=True),
                particles=particles,
            )
        )
    else:
        reporter = SamplerReporter(verbose=True)
        with reporter.create_task_progress() as progress:
            start = time.time()
            handle = reporter.start_task(
                description="Simulating results from model... ",
                progress=progress,
                total=len(particles),
            )
            for particle in particles:
                run_particle(particle=particle)
                reporter.advance(handle)

            end = time.time()
            reporter.print_run_summary(end - start, process_name="Simulations")


parser = argparse.ArgumentParser(description="Run phase 1 projection")
parser.add_argument(
    "-d",
    "--subdirectory-name",
    type=str,
    help="Name of the subdirectory in experiments/phase1/calibration/ to read the calibration results from and run the projection for. The projection results will be written to experiments/phase1/projection/{subdirectory_name}/",
)
parser.add_argument(
    "-f",
    "--force-overwrite",
    action="store_true",
    help="Whether to force overwrite the projection output directory if it already exists. If not set and the output directory already exists, a FileExistsError will be raised.",
)
parser.add_argument(
    "--max-workers",
    type=int,
    default=4,
    help="The maximum number of worker threads to use for running the particles in parallel. If set to 1 or less, the particles will be run serially.",
)
parser.add_argument(
    "--plot-distances",
    action="store_true",
    help="Whether to plot the distance distributions for each generation of the calibration before running the projection.",
)

if __name__ == "__main__":
    args = parser.parse_args()
    main(
        subdirectory_name=args.subdirectory_name,
        force_overwrite=args.force_overwrite,
        max_workers=args.max_workers,
        plot_distances=args.plot_distances,
    )
