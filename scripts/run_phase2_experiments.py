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
from cfa.cloudops import CloudClient
import copy

#========================#
# functions ------------
#========================#
def fill_in_tasks(
        command_vec:list[str],
        mounts: list[dict[str, Any]],
        container_name: str
) -> list[dict[str, Any]]:
    tasks_result:list[dict[str, Any]] = []
    for c_str in command_vec:
        tmp_cmd_dict = {
            "command_line": c_str,
            "mounts": mounts, 
            "full_container_name": container_name,
        }
        tasks_result.append(copy.deepcopy(tmp_cmd_dict))
    return tasks_result


#========================#
# Classes-------------
#========================#

# Steps for calibration
# 1. Define MRP model
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
        #docker_cmd_str = f"sh -c 'set -eux; {docker_str}'"
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
        return docker_str

#========================#
# Setup-------------
#========================#
_ = load_dotenv()
with open("input/calibration_config.json", "r") as file:
    config = json.load(file)

experiment_name = config["experiment_name"]
full_image_container_name = f"{
os.getenv("AZURE_CONTAINER_REGISTRY_ACCOUNT")
}.azurecr.io/{config["repo_name"]}:{config["tag"]}"
pool_input_dir = "input_gce"
pool_output_dir = "output_gce"
test_job_name = f"ixa-epi-covid-job-{experiment_name}"


with open(config['ixa_default_params_file'], "r") as f:
    ixa_defaults = json.load(f)

importation_inputs = {
    "state": config["state"],
    "year": config["year"],
    "symptomatic_reporting_prob": config[
        "symptomatic_reporting_prob"
    ],
}

model_test_inputs = {
    "ixa_inputs": ixa_defaults,
    "config_inputs": config,
    "importation_inputs": importation_inputs
}

task_mounts = [
    {
        "source": pool_output_dir,
        "target": config["output_dir"]
    },
    {
        "source": pool_input_dir,
        "target": config["input_dir"]
    }]

cc = CloudClient()
cc.create_pool(
    pool_name=config["pool_name"],
    container_image_name=full_image_container_name,
    mounts=[
        {"source": config['blob_container_input_name'], "target": pool_input_dir},
        {"source": config["blob_container_output_name"], "target": pool_output_dir}
    ],
    autoscale=True,
    max_autoscale_nodes=10,
    cache_blobfuse=False,
)

cc.delete_blob_folder(
    folder_path=experiment_name,
    container_name=config["blob_container_output_name"],
)

_ = cc.blob_service_client.get_blob_client(
    container=config["blob_container_output_name"],
    blob=f"{experiment_name}/.keep",
).upload_blob(b"", overwrite=True)

#==========================#
# Submit jobs---------
#==========================#
covid_model = CovidModel()
task_cmd_str = covid_model.simulate(model_test_inputs)


cc.upload_files(
    files=["input_1.json"],
    container_name=config["blob_container_input_name"],
    local_root_dir=config["input_dir"],
)
                
# Create job
cc.create_job(
    job_name=test_job_name,
    pool_name=config["pool_name"],
    save_logs_to_blob=config["blob_container_output_name"],
    logs_folder="logs",
    mark_complete_after_tasks_run=True,
    task_retries=1,
)

list_cmd = (
    "sh -c 'set -eux; "
    f"ls /{config["input_dir"]}'"
)
                
task_list = fill_in_tasks(
    command_vec=[list_cmd, task_cmd_str],
    mounts=task_mounts,
    container_name=full_image_container_name)

task_out = cc.add_task_collection(
    tasks = task_list,
    job_name=test_job_name
)


# Monitor job progress
a = cc.monitor_job(
    job_name=test_job_name,
    timeout = 60,
    download_task_output=True,
)

print(task_out)

# Delete job
cc.delete_job(job_name=test_job_name)


## TODO:
# -[x] Shared input files should go into a blob container, given that people_test.csv is small, we can test with copying it to the container
# -[ ] Run an experiment in the cloud, without calibration.
#  +[ ] Run single simulation
# -[ ] Run ABC-SMC in the cloud
# -[ ] Tidy up this script
# 2. Define parameters for azure
# 3. priors, particles to params, outputs to distance
# 4. Run experiment
