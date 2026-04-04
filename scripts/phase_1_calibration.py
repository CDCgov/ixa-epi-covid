import argparse
import json
import os
import pickle
import shutil
import timeit
import warnings
from pathlib import Path

import create_synthetic_population.run as create_synthetic_population
import polars as pl
from calibrationtools import (
    ABCSampler,
    AdaptMultivariateNormalVariance,
    IndependentKernels,
    MultivariateNormalKernel,
    Particle,
    SeedKernel,
)
from dotenv import load_dotenv
from particle_reader import ParticleReader
from requests.exceptions import HTTPError
from us import states

from ixa_epi_covid import (
    CovidModel,
    CovidModelConfig,
    update_epimodel_output_dir,
)

# Run-specific parameters declaration ------------------------------------------------------
TARGET_DATA = pl.DataFrame(
    {
        "t": [75],
        "count": [1],
    }
)


def main(
    config_file: str | Path,
    output_dir: str | Path,
    max_workers: int = 10,
    default_population_size_dev: str = "50_000",
):
    # Load environment files, defaults, and setup configurations ---------------------
    config = CovidModelConfig(
        config_file=config_file,
        target_data=TARGET_DATA,
    )

    # Handle synth population file options ----------------------------------------------
    # Users may specify a SYNTH_POP_FILE of their choosing in the .env file, which will then be copied into the local repo input path for running
    # Otherwise, the default population file is used, and can be created automatically using the create_synthetic_population package if it does not already exist at the specified path
    load_dotenv()
    ixa_overrides = {
        "max_time": config.target_data["t"][0] + config.tolerance_values[0] + 1
    }

    synth_pop_file_env = os.getenv("SYNTH_POP_FILE")
    if synth_pop_file_env and config.use_env_synth_pop_file:
        print(
            f"Using the synth population file specified in environment variable SYNTH_POP_FILE: {synth_pop_file_env}"
        )
        if os.path.exists(synth_pop_file_env):
            filename = os.path.basename(synth_pop_file_env)
            local_synth_pop_file = Path(
                "experiments", "phase1", "input", filename
            )
            if not local_synth_pop_file.exists() and synth_pop_file_env != str(
                local_synth_pop_file
            ):
                os.makedirs(local_synth_pop_file.parent, exist_ok=True)
                shutil.copyfile(synth_pop_file_env, local_synth_pop_file)
        else:
            raise FileNotFoundError(
                f"Synth population file specified in environment variable SYNTH_POP_FILE not found at path: {synth_pop_file_env}"
            )
        ixa_overrides.update(
            {"synth_population_file": str(local_synth_pop_file)}
        )
    else:
        us_state = states.lookup(config.state)
        state_abbr = us_state.abbr
        input_file = Path(
            "input",
            f"synth_pop_people_{state_abbr}_{default_population_size_dev}.csv",
        )
        ixa_overrides.update({"synth_population_file": str(input_file)})
        print(
            f"Creating a default synth population file for {us_state.name}: {input_file}."
        )
        if not os.path.exists(input_file):
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
                    f"Failed to create synthetic population file for {us_state.name} using the year {config.year}. Trying again with the default year 2023."
                )
                create_synthetic_population(
                    [
                        "--size",
                        default_population_size_dev,
                        "--state",
                        state_abbr,
                    ]
                )

    config.update_ixa_params(ixa_overrides)
    # Generate MRP defaults ------------------------------------
    mrp_defaults = config.get_mrp_defaults_for_output(
        output_dir,
        outputs_to_read=["aggregated_deaths_report"],
    )

    # Make the output directory -----------------------------------------------------------------
    # Use the default output directory in config inputs as the base output directory for all outputs
    # Create the output directory, handling the case where it already exists based on the force_overwrite flag
    if os.path.exists(output_dir):
        if config.force_overwrite:
            shutil.rmtree(str(output_dir))
        else:
            raise FileExistsError(
                f"Output directory {output_dir} already exists and force_overwrite is set to False."
            )

    Path(output_dir).mkdir(parents=True, exist_ok=False)

    # Create the priors and perturbation kernels -----------------------------------------------

    with open(config.priors_file, "r") as f:
        priors = json.load(f)

    P: dict[dict, dict] = priors
    K = IndependentKernels(
        [
            MultivariateNormalKernel(list(P["priors"].keys())),
            SeedKernel("seed"),
        ]
    )

    # Model Particle Reader setup -------------------------------------------------------------

    reader = ParticleReader(
        particle_param_names=list(P["priors"].keys()) + ["seed"],
        default_params=mrp_defaults,
    )

    def particles_to_params(
        particle: Particle, reader: ParticleReader = reader
    ):
        particle_params = reader.read_particle(particle=particle)
        # Make particle-specific output directory and update the output path in the parameters accordingly
        output_dir = Path(
            particle_params["config_inputs"]["output_dir"],
            "simulations",
            str(
                particle_params["ixa_inputs"]["epimodel.GlobalParams"]["seed"]
            ),
        )
        output_dir.mkdir(parents=True, exist_ok=False)

        updated_params = update_epimodel_output_dir(
            particle_params, output_dir
        )
        return updated_params

    # Define the distance function ----------------------------------------------------------------

    def outputs_to_distance(
        model_output: dict[str, pl.DataFrame], target_data: pl.DataFrame
    ) -> float:
        """
        Calculates the absoluter error between the observed time of the first death in the model and the reported time of the first death in the data
        and then adds a penalty for the number of deaths reported on that first day over one.

        Args:
            model_output (dict[str, pl.DataFrame]): A dictionary containing the model outputs as Polars DataFrames.
            target_data (pl.DataFrame): The target time of the first death to compare against in column 't' and the number of deaths in column 'count'.
        Returns:
            float: The calculated distance.
        """
        first_death_observed = (
            model_output["aggregated_deaths_report"]
            .filter(pl.col("count") > 0)
            .filter(pl.col("t_upper") == pl.min("t_upper"))
        )
        if first_death_observed.height > 0:
            return abs(
                target_data["t"][0] - first_death_observed.item(0, "t_upper")
            ) + (first_death_observed.height - target_data["count"][0])
        else:
            return 1000.0

    # Initialize the model calibration routine -------------------------------------------------------

    model = CovidModel()

    sampler = ABCSampler(
        generation_particle_count=config.generation_particle_count,
        tolerance_values=config.tolerance_values,
        priors=P,
        perturbation_kernel=K,
        particles_to_params=particles_to_params,
        variance_adapter=AdaptMultivariateNormalVariance(),
        outputs_to_distance=outputs_to_distance,
        target_data=config.target_data,
        model_runner=model,
        entropy=0x2D845A9183A835EC4A777F6C7403A6D0,
    )

    # Execute the sampler ----------------------------------------------------------------------
    start = timeit.default_timer()  # Start the timer
    with warnings.catch_warnings() as _:
        warnings.simplefilter("ignore", category=UserWarning)
        results = sampler.run_parallel(max_workers=max_workers)
    finish = timeit.default_timer()  # Stop the timer
    print(f"Calibration completed in {finish - start:.2f} seconds.")
    print(results)

    # Save results and print diagnostics ---------------------------------------------------------
    diagnostics = results.get_diagnostics()

    print("\nQuantiles for each parameter:")
    print(
        json.dumps(
            {
                k1: {k2: float(v2) for k2, v2 in v1.items()}
                for k1, v1 in diagnostics["quantiles"].items()
            },
            indent=4,
        )
    )

    print("\nCorrelation matrix:")
    print(diagnostics["correlation_matrix"])

    with open(
        Path(output_dir, "results.pkl"),
        "wb",
    ) as fp:
        pickle.dump(results, fp)
    with open(
        Path(output_dir, "config.pkl"),
        "wb",
    ) as fp:
        pickle.dump(config, fp)


parser = argparse.ArgumentParser(
    description="Run phase 1 calibration for ixa-epi-covid."
)
parser.add_argument(
    "--config_file",
    "-c",
    type=str,
    help="Path to the configuration file for phase 1 calibration.",
)
parser.add_argument(
    "--output-dir",
    "-o",
    type=str,
    default="experiments/phase1/calibration/output",
    help="Path to the output directory where results will be saved.",
)
parser.add_argument(
    "--max-workers",
    type=int,
    default=10,
    help="The maximum number of worker processes to use for parallel execution.",
)

parser.add_argument(
    "--default-population-size-dev",
    type=str,
    default="50_000",
    help="The default population size to use for synthetic population generation in development and testing. This is used when a synth population file is not provided via the SYNTH_POP_FILE environment variable and the script needs to generate a synthetic population file automatically. The value should be a string with underscores as thousand separators, e.g. '50_000' for fifty thousand.",
)

if __name__ == "__main__":
    args = parser.parse_args()
    main(
        config_file=args.config_file,
        output_dir=args.output_dir,
        max_workers=args.max_workers,
        default_population_size_dev=args.default_population_size_dev,
    )
