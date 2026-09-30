import argparse
from datetime import date
import json
import os
import pickle
import shutil
import timeit
import warnings
from pathlib import Path
from calibrationtools.azure_batch_executor import AzureBatchExecutor

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
import requests
from requests.exceptions import HTTPError
from us import states
import numpy as np
from particle_reader import ParticleReader

from ixa_epi_covid import (
    CovidModel,
    CovidModelConfig,
    update_epimodel_output_dir,
)
from scipy.stats import poisson


input_dir = Path("input")
output_dir = Path("output")

HOSPITALIZATION_API_URL = "https://healthdata.gov/api/v3/views/g62h-syeh/query.json"

def apply_dtypes(df, dtypes):
    """
    Cast dataframe columns using a dtype dictionary.
    """
    casts = [
        pl.col(column).cast(dtype, strict=False)
        for column, dtype in dtypes.items()
        if column in df.columns
    ]
    return df.with_columns(casts) if casts else df

def load_hospitalization_data():
    """
    Load hospitalization data from the CDC API.
    """
    params = {
            "query": "SELECT * WHERE `state` = 'IN'"
        }
    response = requests.get(HOSPITALIZATION_API_URL, params=params, timeout=60)
    response.raise_for_status()
    df = pl.DataFrame(response.json())
    HOSPITALIZATION_DTYPES = {
    "state": pl.String,
    "date": pl.String,
        **{
            column: pl.Float64
            for column in df.columns
            if column not in {"state", "date"}
        },
    }
    
    df = apply_dtypes(df, HOSPITALIZATION_DTYPES)
    df = df.with_columns(
            pl.col("date").str.to_date("%Y-%m-%dT%H:%M:%S%.3f")
        )
    return df

# -----------------------
# Hospitalization data
hosp_data = (
    load_hospitalization_data()
    .filter(pl.col("state") == "IN")
    .select(["date", "inpatient_beds_used_covid"])
    .with_columns(
        (pl.col("date").cast(pl.Date) - pl.lit(date(2020, 1, 1)))
        .dt.total_days()
        .cast(pl.Float64)
        .alias("t_upper"),
        pl.col("inpatient_beds_used_covid").cast(pl.Float64).alias("inpatient_beds_used_covid"),
    )
    .sort("t_upper")
)
hosp_data = (
    hosp_data.drop("date")
    .rename({"inpatient_beds_used_covid": "count"})
    .with_columns(pl.lit("hospitalizations").alias("data_type"))
)

death_data = (
    pl.read_csv(
        input_dir / "COVID_Deaths_Data.csv",
        try_parse_dates=True,
        infer_schema=False
    )
    .filter(pl.col("state") == "IN")
    .select(["end_date", "new_deaths"])
    .with_columns(
        (pl.col("end_date").str.strptime(pl.Date, "%m/%d/%Y") - pl.lit(date(2020, 1, 1)))
        .dt.total_days()
        .cast(pl.Float64)
        .alias("t_upper"),
        pl.col("new_deaths").cast(pl.Float64).alias("new_deaths")
    )
    .sort("t_upper")
)
death_data = (
    death_data.drop("end_date")
    .rename({"new_deaths": "count"})
    .with_columns(pl.lit("deaths").alias("data_type"))
)

attack_rate_data = pl.DataFrame(
    {
        "t_upper": [120.0, 120.0, 120.0],
        "count": [0.0305, 0.0314, 0.0165],
        "lower": [0.019, 0.019, 0.01],
        "upper": [0.043, 0.05, 0.024],
        "data_type": ["Age0To39AR", "Age40To59AR", "Age60PlusAR"],
    },
    schema={"t_upper": pl.Float64, "count": pl.Float64, "lower": pl.Float64, "upper": pl.Float64, "data_type": pl.String},
)

target_data = pl.concat(
    [
        df.select(
            pl.col("t_upper").cast(pl.Float64),
            pl.col("count").cast(pl.Float64),
            pl.col("data_type").cast(pl.String),
            pl.col("lower").cast(pl.Float64) if "lower" in df.columns else pl.lit(None).cast(pl.Float64).alias("lower"),
            pl.col("upper").cast(pl.Float64) if "upper" in df.columns else pl.lit(None).cast(pl.Float64).alias("upper"),
        )
        for df in (hosp_data, death_data, attack_rate_data)
    ],
    how="vertical",
).sort(["data_type", "t_upper"])

# Run-specific parameters declaration ------------------------------------------------------
TARGET_DATA = target_data


def get_synth_pop_file(
    config: CovidModelConfig
) -> str:
    """
    Obtain the file name of the synthetic population to be used during the calibration routine
    Create the file if it does not exist or throw an error if the file cannot be created

    Args:
        config: CovidModelConfig - configuration dictionary object that contains information on how to build the syntehtic population file or source it from the dotenv file
        default_population_size_dev: str - string cast version of the population size int to be used in the case of generating a toy synthetic population file using `create_synthetic_population`
    Raises:
        FileNotFoundError: if the synthetic population file specified by the dotenv file cannot be found
        HTTPError: if there are http issues with loading the census data, the year is first dropped in an attempt to create the synthetic data
    """

    load_dotenv()
    synth_pop_file_env = os.getenv("SYNTH_POP_FILE")

    if synth_pop_file_env:
        print(
            f"Using the synth population file specified in environment variable SYNTH_POP_FILE: {synth_pop_file_env}"
        )
        if os.path.exists(synth_pop_file_env):
            filename = os.path.basename(synth_pop_file_env)
            local_synth_pop_file = Path(
                "experiments", "phase2", "input", filename
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
        return str(local_synth_pop_file)
    else:
        raise FileNotFoundError(
            "No synthetic population file specified. Set the SYNTH_POP_FILE environment variable in the .env file to a valid synthetic population file path."
        )

def build_executor(max_autoscale_nodes: int | None = None) -> AzureBatchExecutor:
    """Create the Azure executor configured by the smoke-test command line."""
    args = argparse.Namespace(
        base_name='phase2-calibration', 
        registry_server=None, 
        image_name='ixa-epi-covid', 
        image_tag='latest', 
        image_dockerfile='Dockerfile', 
        particle_count=4, 
        chunk_size=1, 
        max_autoscale_nodes=1, 
        study=False, 
        max_concurrent_scenarios=2, 
        detail_log_dir='azure-ixa-epi-covid-study-logs', 
        poll_interval=5.0, 
        max_wait=1800.0, 
        build_image=False, 
        upload_image=False,
        delete_job_after=True, 
        delete_pool_after=True
    )
    return AzureBatchExecutor(
        base_name=args.base_name,
        registry_server=args.registry_server,
        image_name=args.image_name,
        image_tag=args.image_tag,
        max_autoscale_nodes=(
            max_autoscale_nodes
            if max_autoscale_nodes is not None
            else args.max_autoscale_nodes
        ),
        chunk_size=args.chunk_size,
        delete_job_after=args.delete_job_after,
        delete_pool_after=args.delete_pool_after,
        build_image=args.build_image,
        upload_image=args.upload_image,
        image_dockerfile=args.image_dockerfile,
        poll_interval=args.poll_interval,
        max_wait=args.max_wait,
    )

# Define the distance function ----------------------------------------------------------------



# Create the priors and perturbation kernels -----------------------------------------------

PRIORS_FILE = "experiments/phase2/input/priors.json"
# with open(PRIORS_FILE, "r") as f:
#     priors = json.load(f)

# P: dict[dict, dict] = priors


# Model Particle Reader setup -------------------------------------------------------------
# reader = ParticleReader(
#     particle_param_names=list(P["priors"].keys()) + ["seed"],
#     default_params=mrp_defaults,
# )   
# def particles_to_params(
#     particle: Particle, reader: ParticleReader = reader
# ):
#     particle_params = reader.read_particle(particle=particle)
#     # Make particle-specific output directory and update the output path in the parameters accordingly
#     simulations_dir = Path(
#         particle_params["config_inputs"]["output_dir"], "simulations"
#     )
#     # Count existing directories in simulations_dir
#     if not simulations_dir.exists():
#         dir_count = 0
#     else:
#         dir_count = len(os.walk(simulations_dir).__next__()[1])
#     output_dir = Path(
#         simulations_dir,
#         ".".join(
#             [
#                 str(dir_count),
#                 str(
#                     particle_params["ixa_inputs"]["epimodel.GlobalParams"][
#                         "seed"
#                     ]
#                 ),
#             ]
#         ),
#     )
#     output_dir.mkdir(parents=True, exist_ok=False)

#     updated_params = update_epimodel_output_dir(
#         particle_params, output_dir
#     )
#     return updated_params

def main(
    config_file: CovidModelConfig ,
    output_dir: str | Path,
    max_workers: int = 10,
    default_population_size_dev: str = "50_000",
):   

    # Load environment files, defaults, and setup configurations ---------------------
    config = CovidModelConfig(
        config_file=config_file,
        target_data=TARGET_DATA,
    )

    # Update ixa overrides ----------------------------------------------
    # Users may specify a SYNTH_POP_FILE of their choosing in the .env file, which will then be copied into the local repo input path for running
    # Otherwise, the default population file is used, and can be created automatically using the create_synthetic_population package if it does not already exist at the specified path
    ixa_overrides = {}

    synth_pop_file = get_synth_pop_file(config)
    ixa_overrides.update({"synth_population_file": synth_pop_file})
    config.update_ixa_params(ixa_overrides)
    config.set_priors(priors_file=PRIORS_FILE)
    config.set_perturbation_kernel()
    config.set_mrp_defaults(output_dir=output_dir, outputs_to_read = ["aggregated_deaths_report", "current_hospitalizations_report", "aggregated_attack_rate_report"])
    config.set_reader()
    
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


    # Initialize the model calibration routine -------------------------------------------------------

    model = CovidModel()

    sampler = ABCSampler(
        generation_particle_count=config.generation_particle_count,
        tolerance_values=config.tolerance_values,
        priors=config.priors,
        perturbation_kernel=config.perturbation_kernel,
        particles_to_params=config.particles_to_params,
        variance_adapter=AdaptMultivariateNormalVariance(),
        outputs_to_distance=model.outputs_to_distance,
        target_data=config.target_data,
        model_runner=model,
        entropy=0x2D845A9183A835EC4A777F6C7403A6D0,
    )

    # Execute the sampler ----------------------------------------------------------------------
    start = timeit.default_timer()  # Start the timer
    with warnings.catch_warnings() as _:
        warnings.simplefilter("ignore", category=UserWarning)
        results = sampler.run(
            execution="azure_batch", cloud_executor=build_executor()
        )
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
    description="Run phase 2 calibration for ixa-epi-covid."
)
parser.add_argument(
    "--config_file",
    "-c",
    type=str,
    help="Path to the configuration file for phase 2 calibration.",
)
parser.add_argument(
    "--output-dir",
    "-o",
    type=str,
    default="experiments/phase2/calibration/output",
    help="Path to the output directory where results will be saved.",
)
# parser.add_argument(
#     "--max-workers",
#     type=int,
#     default=2,
#     help="The maximum number of worker processes to use for parallel execution.",
# )


if __name__ == "__main__":
    args = parser.parse_args()
    main(
        config_file = args.config_file,
        output_dir=args.output_dir,
    )
