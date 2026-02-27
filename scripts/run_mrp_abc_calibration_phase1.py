from __future__ import annotations

import json
import os
import shutil
from pathlib import Path

import numpy as np
import polars as pl
from calibrationtools.perturbation_kernel import (
    IndependentKernels,
    MultivariateNormalKernel,
    SeedKernel,
)
from calibrationtools.prior_distribution import (
    IndependentPriors,
    SeedPrior,
    UniformPrior,
)
from calibrationtools.sampler import ABCSampler
from calibrationtools.variance_adapter import AdaptMultivariateNormalVariance
from covid_model import Covid_Ixa_Model
from mrp.api import apply_dict_overrides

##================================#
## Configuration ------------
##================================#
# Config variables
config = {
    "parameter_key_name": "epimodel\\.GlobalParams",
    "particle_count": 250,
    "seed": 123,
    "error_array": [10.0, 5.0],
    "output_dir": "experiments/phase1_calibration/output",
    "exe_file": "./target/release/ixa-epi-covid",
    "default_params_file": "./input/input.json",
    "target_csv": "./input/target_data_phase1.csv",
}

output_dir = Path(config["output_dir"])
if os.path.exists(output_dir):
    shutil.rmtree(str(output_dir))

output_dir.mkdir(parents=True, exist_ok=True)    
input_file_names = [
    config[x] for x in ["exe_file", "default_params_file", "target_csv"]
]
for p in input_file_names:
    if not Path(p).exists():
        raise FileNotFoundError(f"Missing required file: {p}")

with open(config["default_params_file"], "r") as f:
    default_params_dict = json.load(f)

# Target data with cumulative
target_data = (
    pl.read_csv(config["target_csv"])
    .sort("t")
    .with_columns(pl.col("cases").cum_sum().alias("data_cum"))
)

##================================#
## Setup model ------------
##================================#
model_mrp_input = {
    "model_config": {
        "output_dir": config["output_dir"],
        "exe_file": config["exe_file"],
    },
    "model_inputs": default_params_dict,
}


##================================#
## Functions ------------
##================================#
def particles_to_params(particle, **kwargs):
    base_inputs = kwargs.get("base_inputs")
    particle_dict = {"model_inputs": {"epimodel.GlobalParams": dict(particle)}}
    model_params = apply_dict_overrides(base_inputs, particle_dict)
    model_params["model_inputs"]["epimodel.GlobalParams"]["seed"] = int(
        model_params["model_inputs"]["epimodel.GlobalParams"]["seed"]
    )
    return model_params


def outputs_to_distance(model_output, target_data):
    symptom_report = (
        model_output.filter(pl.col("symptoms") == "Symptomatic")
        .group_by("t")
        .agg(pl.col("count").sum().alias("symptom_count"))
        .sort("t")
    )
    time_df = pl.DataFrame(
        {"time": pl.arange(0, model_output["t"].max(), eager=True)}
    ).with_columns(pl.col("time").cast(pl.Float64).alias("t"))

    report_output_df = (
        symptom_report.join(time_df.select("t"), on="t", how="right")
        .with_columns(pl.col("symptom_count").fill_null(0.0))
        .with_columns(pl.col("symptom_count").cum_sum().alias("symptom_cum"))
    )
    join_df = report_output_df.join(
        target_data,
        on="t",
        how="right",
    ).with_columns(
        (pl.col("symptom_cum") - pl.col("data_cum")).abs().alias("error")
    )
    distance_error = join_df["error"].sum()
    return distance_error


##================================#
## Priors and Kernels ------------
##================================#

P = IndependentPriors(
    [
        UniformPrior("probability_importation_infectious", 0.0, 1.0),
        UniformPrior("probability_symptoms", 0.0, 1.0),
        SeedPrior("seed"),
    ]
)

K = IndependentKernels(
    [
        MultivariateNormalKernel(
            [p.params[0] for p in P.priors if not isinstance(p, SeedPrior)],
        ),
        SeedKernel("seed"),
    ]
)

V = AdaptMultivariateNormalVariance()

##===================================#
## Run ABC-SMC
##===================================#
covid_model = Covid_Ixa_Model()
sampler = ABCSampler(
    generation_particle_count=config["particle_count"],
    tolerance_values=config["error_array"],
    priors=P,
    perturbation_kernel=K,
    variance_adapter=V,
    particles_to_params=particles_to_params,
    outputs_to_distance=outputs_to_distance,
    target_data=target_data,
    model_runner=covid_model,
    seed=config[
        "seed"
    ],  # Propagation of seed must be SeedSequence not int for proper pseudorandom draws
)

sampler.run(base_inputs=model_mrp_input)

##===================================#
## Get results
##===================================#
# Print IQR of param1 in the posterior particles
posterior_particles = sampler.get_posterior_particles()
pimp_values = [
    p.state["probability_importation_infectious"]
    for p in posterior_particles.all_particles
]
psymp_values = [
    p.state["probability_symptoms"] for p in posterior_particles.all_particles
]

print(
    f"param importations(25-75):{np.percentile(pimp_values, 25)} - {np.percentile(pimp_values, 75)}"
)
print(
    f"param symptoms(25-75):{np.percentile(psymp_values, 25)} - {np.percentile(psymp_values, 75)}"
)
