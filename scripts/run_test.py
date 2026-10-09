from cfa.cloudops import CloudClient
from dotenv import load_dotenv
import json
import os
from typing import Any
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

def build_command_str_with_args(cmd_args: list[str]) -> list[str]:
    command_out:list[str] = []
    for c in cmd_args:
        cmd_str = (
            "sh -c 'set -eux; "
            f"printf \"%s\\n\" \"hello, world\" > /{c}; "
        )
        command_out.append(cmd_str)
    return command_out

#========================#
# setup-------------
#========================#
_ = load_dotenv()
cc = CloudClient()
with open("input/calibration_config.json", "r") as file:
    config = json.load(file)
    
full_image_container_name = f"{
os.getenv('AZURE_CONTAINER_REGISTRY_ACCOUNT')
}.azurecr.io/{config["repo_name"]}:{config["tag"]}"
pool_input_dir = "input_gce"
pool_output_dir = "output_gce"
test_job_name = "test-ixa-epi-covid"
# Create a pool to run the calibration

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
# Create job
cc.create_job(
    job_name=test_job_name,
    pool_name=config["pool_name"],
    save_logs_to_blob=config["blob_container_output_name"],
    logs_folder="logs",
    mark_complete_after_tasks_run=True,
    task_retries=1,
)

cmd_arg_list: list[str] = [
    f"{config["output_dir"]}/output_test_1.txt",
    f"{config["output_dir"]}/output_test_2.txt"
]
                
task_mounts = [
    {
        "source": pool_output_dir,
        "target": config["output_dir"]
    },
    {
        "source": pool_input_dir,
        "target": config["input_dir"]
    }]

task_list = fill_in_tasks(
    command_vec=build_command_str_with_args(cmd_arg_list),
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

## After setup
# - [x] Create a list of tasks to run
# - [x] Submit the tasks to run
# - [x] Wait for jobs to finish
# - [ ] Gather results back
# - [ ] Delete pool
# - [ ] Delete storage
