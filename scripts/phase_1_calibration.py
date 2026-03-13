import json
import os
import pickle
import shutil
import timeit
from pathlib import Path

import polars as pl
from calibrationtools import (
    ABCSampler,
    AdaptMultivariateNormalVariance,
    IndependentKernels,
    MultivariateNormalKernel,
    SeedKernel,
)
from mrp.api import apply_dict_overrides

from ixa_epi_covid import CovidModel

# Run-specific parameters declaration ------------------------------------------------------
# Default model and parameters
exe_file = Path("target", "release", "ixa-epi-covid")
output_dir = Path("experiments", "phase1", "calibration", "output")
default_ixa_params_file = Path(
    "experiments", "phase1", "input", "default_params.json"
)
ixa_overrides = {
    "synth_population_file": "/mnt/S_CFA_Predict/team-CMEI/synthetic_populations/cbsa_all_work_school_household_2020-04-24/cbsa_all_work_school_household/IN/Bloomington IN.csv",
    "first_death_terminates_run": True,
}
force_overwrite = False
outputs_to_read = ["incidence_report"]

# State importation model declaration parameters
state = "Indiana"
year = 2020
symptomatic_reporting_prob_default = 0.5

# Calibration inputs
priors_file = Path("experiments", "phase1", "input", "priors.json")
tolerance_values = [2.0, 0.1]
generation_particle_count = 500
target_data = 75


# Output processing function for calibration
def outputs_to_distance(
    model_output: dict[str, pl.DataFrame], target_data: int
):
    first_death_observed = (
        model_output["incidence_report"]
        .filter((pl.col("event") == "Dead") & (pl.col("count") > 0))
        .filter(pl.col("t_upper") == pl.min("t_upper"))
    )
    if first_death_observed.height > 0:
        return abs(target_data - first_death_observed.item(0, "t_upper"))
    else:
        return 1000


# Load environment files, defaults, and setup configurations ---------------------
with open(default_ixa_params_file, "r") as f:
    default_params = json.load(f)

ixa_overrides.update(
    {
        "epimodel.GlobalParams": {
            "max_time": target_data + tolerance_values[0] + 1
        }
    }
)

default_params = apply_dict_overrides(
    default_params, {"epimodel.GlobalParams": ixa_overrides}
)

mrp_defaults = {
    "ixa_inputs": default_params,
    "config_inputs": {
        "exe_file": str(exe_file),
        "output_dir": str(output_dir),
        "force_overwrite": force_overwrite,
        "outputs_to_read": outputs_to_read,
    },
    "importation_inputs": {
        "state": state,
        "year": year,
        "symptomatic_reporting_prob": symptomatic_reporting_prob_default,
    },
}

output_dir = Path(mrp_defaults["config_inputs"]["output_dir"])
if os.path.exists(output_dir):
    if force_overwrite:
        shutil.rmtree(str(output_dir))
    else:
        raise FileExistsError(
            f"Output directory {output_dir} already exists and force_overwrite is set to False."
        )

output_dir.mkdir(parents=True, exist_ok=False)

# Create the model and sampler objects ------------------------------------------------
with open(priors_file, "r") as f:
    priors = json.load(f)

P: dict[dict, dict] = priors
K = IndependentKernels(
    [
        MultivariateNormalKernel(list(P["priors"].keys())),
        SeedKernel("seed"),
    ]
)

model = CovidModel()

sampler = ABCSampler(
    generation_particle_count=generation_particle_count,
    tolerance_values=tolerance_values,
    priors=P,
    perturbation_kernel=K,
    variance_adapter=AdaptMultivariateNormalVariance(),
    outputs_to_distance=outputs_to_distance,
    target_data=target_data,
    model_runner=model,
    seed=123,
)

# Execute the sampler ----------------------------------------------------------------------
start = timeit.default_timer()  # Start the timer
results = sampler.run(default_params=mrp_defaults)
finish = timeit.default_timer()  # Stop the timer
print(f"Calibration completed in {finish - start:.2f} seconds.")
print(results)

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
    Path("experiments", "phase1", "calibration", "output", "results.pkl"),
    "wb",
) as fp:
    pickle.dump(results, fp)
