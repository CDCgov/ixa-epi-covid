import subprocess
from pathlib import Path
import json

from cfa.cloudops import CloudClient
from dotenv import load_dotenv
import os

def build_and_upload_with_podman(
    registry_name: str,
    repo_name: str,
    tag: str,
    path_to_dockerfile: str = "./Dockerfile",
) -> str:
    image_name = f"{registry_name}.azurecr.io/{repo_name}:{tag}"
    dockerfile = Path(path_to_dockerfile)
    if not dockerfile.is_file():
        raise FileNotFoundError(f"Dockerfile not found: {dockerfile}")

    subprocess.run(
        ["podman", "build", "-f", str(dockerfile), "-t", image_name, "."],
        check=True,
    )

    token = subprocess.run(
        [
            "az",
            "acr",
            "login",
            "--name",
            registry_name,
            "--expose-token",
            "--query",
            "accessToken",
            "--output",
            "tsv",
        ],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout.strip()
    if not token:
        raise RuntimeError("Azure CLI returned an empty ACR access token.")

    subprocess.run(
        [
            "podman",
            "login",
            f"{registry_name}.azurecr.io",
            "--username",
            "00000000-0000-0000-0000-000000000000",
            "--password-stdin",
        ],
        input=token,
        check=True,
        text=True,
    )
    subprocess.run(["podman", "push", image_name], check=True)
    return image_name

def setup_blob_containers(
        client:CloudClient,
        input_container: str,
        output_container: str,
        files_to_send_to_input: list[str],
):
    client.create_blob_container(input_container)
    client.create_blob_container(output_container)
    client.upload_files(files_to_send, container_name=blob_container_name)
    print(client.list_blob_files(input_container))
    print(client.list_blob_files(output_container))

## Setup-----------    
load_dotenv()
cc = CloudClient()
with open("input/calibration_config.json", "r") as file:
    config = json.load(file)
files_to_send = ["./input/in.csv", "input/people_test.csv"]

setup_blob_containers(
    client= cc,
    input_container=config["blob_container_input_name"],
    output_container=config["blob_container_output_name"],
    files_to_send_to_input=files_to_send)

build_and_upload_with_podman(
    registry_name=os.getenv('AZURE_CONTAINER_REGISTRY_ACCOUNT'),
    repo_name=config["repo_name"],
    tag=config["tag"],
    path_to_dockerfile="./" + config["dockerfile"],
)
