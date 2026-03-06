import json
import os
import pickle
import shutil
from pathlib import Path

import polars as pl
from calibrationtools import (
    ABCSampler,
    AdaptMultivariateNormalVariance,
    IndependentKernels,
    MultivariateNormalKernel,
    SeedKernel,
)

from ixa_epi_covid import CovidModel

with open(Path("experiments", "phase1", "input", "priors.json"), "r") as f:
    priors = json.load(f)

with open(
    Path("experiments", "phase1", "input", "default_params.json"), "r"
) as f:
    default_params = json.load(f)

mrp_defaults = {
    "ixa_inputs": default_params,
    "config_inputs": {
        "exe_file": "./target/release/ixa-epi-covid",
        "output_dir": "./experiments/phase1/calibration/output",
        "force_overwrite": True,
    },
    "importation_inputs": {"state": "Indiana", "year": 2020},
}

output_dir = Path(mrp_defaults["config_inputs"]["output_dir"])
if (
    os.path.exists(output_dir)
    and mrp_defaults["config_inputs"]["force_overwrite"]
):
    shutil.rmtree(str(output_dir))

output_dir.mkdir(parents=True, exist_ok=False)

P = priors
K = IndependentKernels(
    [
        MultivariateNormalKernel(
            [p for p in P["priors"].keys()],
        ),
        SeedKernel("seed"),
    ]
)

model = CovidModel()


def outputs_to_distance(model_output: pl.DataFrame, target_data: int):
    first_death_observed = model_output.filter(
        (pl.col("event") == "Dead") & (pl.col("count") > 0)
    ).filter(pl.col("t_upper") == pl.min("t_upper"))
    if first_death_observed.height > 0:
        return abs(target_data - first_death_observed.item(0, "t_upper"))
    else:
        return 1000


sampler = ABCSampler(
    generation_particle_count=1000,
    tolerance_values=[30.0, 20.0, 10.0, 5.0, 2.0, 0.0],
    priors=P,
    perturbation_kernel=K,
    variance_adapter=AdaptMultivariateNormalVariance(),
    outputs_to_distance=outputs_to_distance,
    target_data=65,
    model_runner=model,
    seed=123,
)

results = sampler.run(
    default_params=mrp_defaults,
    parameter_headers=["ixa_inputs", "epimodel.GlobalParams"],
)

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
