import asyncio
import pickle
from pathlib import Path

from calibrationtools import CalibrationResults, Particle
from particle_reader import ParticleReader

from ixa_epi_covid import CovidModel, update_epimodel_output_dir
from ixa_epi_covid.config_parser import CovidModelConfig

# Run specific parameters declaration --------------------------------------------------------------------------------
rerun_particle_count = 10

# Load the calibration --------------------------------------------------------------------------------
calibration_output_dir = Path("experiments", "phase1", "calibration", "output")
with open(calibration_output_dir / "config.pkl", "rb") as f:
    config: CovidModelConfig = pickle.load(f)

with open(calibration_output_dir / "results.pkl", "rb") as f:
    results: CalibrationResults = pickle.load(f)

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
    output_dir = Path(
        particle_params["config_inputs"]["output_dir"],
        "simulations",
        str(particle_params["ixa_inputs"]["epimodel.GlobalParams"]["seed"]),
    )
    output_dir.mkdir(parents=True, exist_ok=False)

    updated_params = update_epimodel_output_dir(particle_params, output_dir)
    return updated_params


def run_particle(particle: Particle):
    particle_params = particles_to_params(particle=particle)
    model.simulate(particle_params)


async def simulate_particles(particles: list[Particle]):
    description = f"Projecting from posterior "
    reporter
    request
    with reporter.create_weight_progress() as progress:
        handle = self.config.reporter.start_collection_task(
            progress=progress,
            description=description,
            total=self.config.generation_particle_count,
        )
        assert request.executor is not None

        loop = asyncio.get_running_loop()
        worker = partial(
            run_particle,
            particle_kwargs=request.particle_kwargs,
        )
        particle_chunks = [
            proposed_particles[index : index + request.chunksize]
            for index in range(0, len(proposed_particles), request.chunksize)
        ]
        tasks = []
        for chunk in particle_chunks:
            task = loop.run_in_executor(request.executor, worker, chunk)
            tasks.append((task, chunk))

        attempts = 0
        try:
            for task, chunk in tasks:
                chunk_results = await task
                attempts += self._accept_particle_batch(
                    generation=request.generation,
                    proposed_population=state.proposed_population,
                    proposed_particles=chunk,
                    errs=chunk_results,
                )
                if (
                    state.proposed_population.size
                    >= self.config.generation_particle_count
                ):
                    break
        finally:
            for task, _ in tasks:
                task.cancel()

        generation_stats = self._build_generation_stats(
            request=request,
            state=state,
        )
        reporter.print_timing_summary()
