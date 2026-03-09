import pickle
from calibrationtools.calibration_results import CalibrationResults
from pathlib import Path
import json
from calibrationtools import default_particle_reader
import tempfile
import polars as pl
from ixa_epi_covid import CovidModel
import matplotlib.pyplot as plt
import seaborn as sns
import os


with open(Path("experiments","phase1","calibration","output","results.pkl"), "rb") as fp:
    results: CalibrationResults = pickle.load(fp)

diagnostics = results.get_diagnostics()
# print(
#     json.dumps(
#         {
#             k1: {k2: float(v2) for k2, v2 in v1.items()}
#             for k1, v1 in diagnostics["quantiles"].items()
#         },
#         indent=4,
#     )
# )
# Re-generating a random sample of parameter sets from posterior

particles = results.sample_posterior_particles(n=100)
default_params_file = Path("experiments", "phase1","input","default_params.json")

with open(default_params_file, "rb") as fp:
    default_params = json.load(fp)

mrp_defaults = {
    'ixa_inputs': default_params,
    "config_inputs": {
        "exe_file": "./target/release/ixa-epi-covid",
        "output_dir": "./experiments/phase1/calibration/output",
        "force_overwrite": True,
    },
    "importation_inputs": {"state": "Indiana", "year": 2020},
}

uniq_id = 0
model = CovidModel()
importation_curves = []
prevalence_data = []

with tempfile.TemporaryDirectory() as tmpdir:
    for p in particles:
        model_inputs = default_particle_reader(
            p, 
            default_params=mrp_defaults, 
            parameter_headers=["ixa_inputs","epimodel.GlobalParams"]
        )
        
        model_inputs["config_inputs"]["output_dir"] = str(Path(tmpdir, f'{uniq_id}'))
        os.makedirs(model_inputs["config_inputs"]["output_dir"], exist_ok=True)
        importation_path = Path(tmpdir, f'{uniq_id}', "importation_timeseries.csv")
        
        model_inputs["ixa_inputs"]["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"] = str(importation_path)
        model.simulate(model_inputs)
        prevalence_data.append(
            pl.read_csv(Path(tmpdir, f'{uniq_id}', model_inputs["ixa_inputs"]['epimodel.GlobalParams']["prevalence_report"]["filename"])).with_columns(
                pl.lit(uniq_id).alias("id")
            )
        )
        importation_curves.append(
            pl.read_csv(importation_path).with_columns(
                pl.lit(uniq_id).alias("id")
            )
        )
        uniq_id += 1
print(len(importation_curves))
importations = pl.concat(importation_curves)
deaths = pl.concat(prevalence_data).filter(pl.col('symptom_status') == 'Dead').group_by('t', 'id').agg(pl.sum('count'))

sns.lineplot(
    data = importations,
    x="time",
    y="imported_infections",
    units="id",
    estimator=None,
    alpha=0.05
)
plt.show()

sns.lineplot(
    data = deaths,
    x="t",
    y="count",
)
plt.show()
sns.histplot(
    data = deaths.filter(pl.col('count') > 0).group_by('id').agg(pl.min('t')),
    x = 't',
)
plt.show()