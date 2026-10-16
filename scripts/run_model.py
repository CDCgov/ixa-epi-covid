import json
import os
import timeit
import warnings
from pathlib import Path

import polars as pl
from dotenv import load_dotenv
import requests
from requests.exceptions import HTTPError
from us import states
import numpy as np

import json
import subprocess
from typing import Any

from importation import ImportationModel, get_linelist_data
from mrp import MRPModel
from cfa.cloudops import CloudClient
import copy

#========================#
# Classes-------------
#========================#
class CovidModel(MRPModel):
    def run(self):
        pass

    @staticmethod
    def run_in_docker(
            cmd_args: list[str],
            mount_output_dir: Path,
            mount_input_dir: Path,
            container_mount: str = "/",
            container_output: str = "output",
            container_input: str = "input",
    ) -> subprocess.CompletedProcess:
        docker_cmd = [
            "podman", "run", "--rm",
            "-v", f"{mount_output_dir.resolve()}:{container_mount}/{container_output}",
            "-v", f"{mount_input_dir.resolve()}:{container_mount}/{container_input}",
            "-w", container_mount,
            "ixa-epi-covid:latest",
            "./target/release/ixa-epi-covid",
            *cmd_args,
        ]
        try: 
            result = subprocess.run(docker_cmd, capture_output=True, check=True)
            return result
        except subprocess.CalledProcessError as e:
            print("STDOUT:", e.stdout.decode())
            print("STDERR:", e.stderr.decode())
            raise

    @staticmethod
    def build_docker_cmd_str(
            cmd_args: list[str],
            mount_output_dir: Path,
            mount_input_dir: Path,
            container_mount: str = "/app",
            container_output: str = "output",
            container_input: str = "input",
            exec_path: str = "./target/release/ixa-epi-covid",
            repo_name: str = "ixa-epi-covid-test",
            tag: str = "latest",
    ) -> str:
        docker_cmd = [
            exec_path,
            *cmd_args,
        ]
        docker_str = " ".join(docker_cmd)
        return docker_str

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
        timeseries_data.write_csv(ixa_inputs["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"])

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> str:
        ixa_inputs = copy.deepcopy(x=model_inputs["ixa_inputs"])
        config_inputs = model_inputs["config_inputs"]
        importation_inputs = model_inputs["importation_inputs"]
        # Write command to call the ixa model binaries
        input_file_path = Path(config_inputs["input_dir"], "input_1.json")
        ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
            ixa_inputs["epimodel.GlobalParams"]["seed"]
        )
        
        CovidModel.simulate_imports(ixa_inputs, importation_inputs)
        pop_file = ixa_inputs["epimodel.GlobalParams"]["synth_population_file"]
        imports_file = ixa_inputs["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"]
        input_mount = f"/{config_inputs['input_dir']}"
        ixa_inputs["epimodel.GlobalParams"]["synth_population_file"] = (
            f"{input_mount}/{Path(pop_file).name}"
        )
        ixa_inputs["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"] = (
            f"{input_mount}/{Path(imports_file).name}"
        )
        
        with open(input_file_path, "w") as f:
            json.dump(ixa_inputs, f, indent=4)

        cmd_args = [
            "--config", f"/{config_inputs['input_dir']}/{input_file_path.name}",
            "--output", f"/{config_inputs['output_dir']}/{config_inputs['experiment_name']}",
            "--force-overwrite",
            ##"--no-stats",
        ]

        docker_str = CovidModel.build_docker_cmd_str(
                cmd_args,
                mount_output_dir=config_inputs["output_dir"],
                mount_input_dir=config_inputs["input_dir"],
                container_output = "output",
                container_input = "input"
            )
        result = subprocess.run(docker_str, capture_output=True, check=True)
        return docker_str

def main():
    covid_model = CovidModel()
    task_cmd_str = covid_model.simulate(model_test_inputs)

if __name__ == "__main__":
    main()
