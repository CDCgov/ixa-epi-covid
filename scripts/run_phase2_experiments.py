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
#covid_model = CovidModel()
#task_cmd_str = covid_model.simulate(model_test_inputs)


cc.upload_files(
    files=["./input/calibration_config.json", "input_1.json"],
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
