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

import json
import subprocess
from typing import Any

from importation import ImportationModel, get_linelist_data
from mrp import MRPModel


# Steps for calibration
# 1. Define MRP model
class CovidModel(MRPModel):
    def run(self):
        pass

    @staticmethod
    def run_in_docker(
            cmd_args: list[str], mount_dir: Path,
            container_mount: str = "/app",
            container_output: str = "output",
    ) -> subprocess.CompletedProcess:
        docker_cmd = [
            "docker", "run", "--rm",
            "-v", f"{mount_dir.resolve()}:{container_mount}/{container_output}",
            "-w", container_mount,
            "ixa-epi-covid:latest",
            "./target/release/ixa-epi-covid",
            *cmd_args,
        ]
        try: 
            result = subprocess.run(docker_cmd, capture_output=True, check=True)
        except subprocess.CalledProcessError as e:
            print("STDOUT:", e.stdout.decode())
            print("STDERR:", e.stderr.decode())
            raise

    @staticmethod
    def simulate_imports(
            ixa_inputs: dict[str, Any],
            importation_inputs: dict[str, Any]
    ):
        # Generate the imports time series from ixa parameters ----
        (p_mild_given_infect, p_severe_given_mild,
         p_critical_given_severe, p_dead_given_critical,
         importation_filename, seed,
         ) = (
             ixa_inputs['epimodel.GlobalParams'][k]
            for k in [
                    "probability_mild_given_infect",
                    "probability_severe_given_mild",
                    "probability_critical_given_severe",
                    "probability_dead_given_critical",
                    "imported_cases_timeseries", "seed"]
         )
        n_age_groups = len(
            ixa_inputs["epimodel.GlobalParams"]["symptom_age_groups"]
        )
        # Calculate the prob that an inidivdual dies given symptomatic
        # This assumes that individuals are evenly distributed by age group
        case_fatality_ratio = (
            sum(p_severe_given_mild.values()) / n_age_groups *
            sum(p_critical_given_severe.values()) / n_age_groups *
            sum(p_dead_given_critical.values()) / n_age_groups
        )

        importation_params = {
            "symptomatic_reporting_prob": importation_inputs["symptomatic_reporting_prob"],
            "case_fatality_ratio": case_fatality_ratio,
            "proportion_asymptomatic": 1.0 - p_mild_given_infect,
        }

        # Create the model object
        importation_model = ImportationModel(
            data=get_linelist_data(),
            parameters=importation_params,
            national_model="multinomial",
            state_model="proportional",
            seed=seed,  # Optional argument to set the model seed
        )
        # Generate timeseries from the model object for state and optional year
        timeseries_data = importation_model.sample_state_importation_incidence(
            state=importation_inputs["state"],
            year=importation_inputs.get("year"),
        )

        # Store timeseries at appropriate location accessible to ixa
        timeseries_data.write_csv(importation_filename["filename"])

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> pl.DataFrame:
        ixa_inputs = model_inputs["ixa_inputs"]
        config_inputs = model_inputs["config_inputs"]
        importation_inputs = model_inputs["importation_inputs"]
        docker_prefix_dir = "/app"
        # Write command to call the ixa model binaries
        input_file_path = Path(config_inputs["output_dir"], "input.json")
        ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
            ixa_inputs["epimodel.GlobalParams"]["seed"]
        )
        
        CovidModel.simulate_imports(ixa_inputs, importation_inputs)
        pop_file = ixa_inputs["epimodel.GlobalParams"]["synth_population_file"]
        imports_file = ixa_inputs["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"]
        ixa_inputs["epimodel.GlobalParams"]["synth_population_file"] = f"{docker_prefix_dir}/{pop_file}"
        ixa_inputs["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"] = f"{docker_prefix_dir}/{imports_file}"
        
        with open(input_file_path, "w") as f:
            json.dump(ixa_inputs, f, indent=4)

        cmd_args = [
            "--config", str(input_file_path),
            "--output", config_inputs["output_dir"],
            "--force-overwrite",
            "--no-stats",
        ]


        try:
            CovidModel.run_in_docker(cmd_args, mount_dir=Path(config_inputs["output_dir"]))
        except subprocess.CalledProcessError as e:
            print("Error running the ixa model:")
            print("Command:", " ".join(cmd_args))
            print("Return code:", e.returncode)
            print("Standard error:", e.stderr)
            raise e

        # Read the incidence report from the location and return as a DataFrame
        outputs = {}
        for output in config_inputs["outputs_to_read"]:
            fp = ixa_inputs["epimodel.GlobalParams"][output]["filename"]
            if Path(config_inputs["output_dir"], fp).exists():
                outputs.update(
                    {
                        output: pl.read_csv(
                            Path(config_inputs["output_dir"], fp)
                        )
                    }
                )
            elif Path(fp).exists():
                outputs.update({output: pl.read_csv(Path(fp))})
            else:
                raise FileNotFoundError(
                    f"Expected output file {fp} not found. Looked in {config_inputs['output_dir']}"
                )
        return outputs


# 2. Define parameters for azure
# 3. priors, particles to params, outputs to distance
# 4. Run experiment

config = {
    "exe_file": "./target/release/ixa-epi-covid",
    "force_overwrite": True,
    "state": "Indiana",
    "year": 2020,
    "symptomatic_reporting_prob": 0.5,
    "priors_file": "./experiments/phase2/input/priors.json",
    "generation_particle_count": 3,
    "tolerance_values": [10000000.0],
    "use_env_synth_pop_file": True,
    "ixa_default_params_file": "./experiments/phase2/input/default_params.json",
    "output_dir": "output",
    "outputs_to_read": ["aggregated_deaths_report",
                        "current_hospitalizations_report",
                        "attack_rate_report"]
}

importation_inputs = {
    "state": config["state"],
    "year": config["year"],
    "symptomatic_reporting_prob": config[
        "symptomatic_reporting_prob"
    ],
}
with open(config['ixa_default_params_file'], "r") as f:
    ixa_defaults = json.load(f)
ixa_defaults["epimodel.GlobalParams"]["synth_population_file"] = "output/people_test.csv"
ixa_defaults["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"] = "output/importation_test.csv"

# THE FILES ARE READ FROM A BLOB STORAGE. PROBABLY. 
model_test_inputs = {
    "ixa_inputs": ixa_defaults,
    "config_inputs": config,
    "importation_inputs": importation_inputs
}

covid_model = CovidModel()
covid_model.simulate(model_test_inputs)


## TODO:
# -[ ] Shared input files should go into a blob container, given that people_test.csv is small, we can test with copying it to the container
# -[ ] Run an experiment in the cloud, without calibration.
# -[ ] Run ABC-SMC in the cloud
# -[ ] Tidy up this script
