from __future__ import annotations

from pathlib import Path

from calibrationtools.cloud.backend import (
    DEFAULT_CLOUD_EXECUTOR_BACKEND,
    DEFAULT_CLOUD_RUNNER_BACKEND,
)
from calibrationtools.cloud.config import CloudRuntimeSettings
from calibrationtools.cloud.config import (
    load_cloud_runtime_settings as _load_cloud_runtime_settings,
)

DEFAULT_CLOUD_RUNTIME_SETTINGS = CloudRuntimeSettings(
    keyvault="cfa-predict",
    local_image="ixa-epi-covid-cloud",
    repository="ixa-epi-covid-cloud",
    task_mrp_config_path="/app/ixa_epi_covid.mrp.task.toml",
    pool_prefix="ixa-epi-covid-cloud",
    job_prefix="ixa-epi-covid-cloud",
    input_container_prefix="ixa-epi-covid-cloud-input",
    output_container_prefix="ixa-epi-covid-cloud-output",
    logs_container_prefix="ixa-epi-covid-cloud-logs",
    task_slots_per_node=50,
    dispatch_buffer=1000,
)


def load_cloud_runtime_settings(
    config_path: str | Path,
) -> CloudRuntimeSettings:
    return _load_cloud_runtime_settings(
        config_path,
        defaults=DEFAULT_CLOUD_RUNTIME_SETTINGS,
    )


cloud_runner_backend = DEFAULT_CLOUD_RUNNER_BACKEND
cloud_executor_backend = DEFAULT_CLOUD_EXECUTOR_BACKEND

__all__ = [
    "CloudRuntimeSettings",
    "DEFAULT_CLOUD_RUNTIME_SETTINGS",
    "cloud_executor_backend",
    "cloud_runner_backend",
    "load_cloud_runtime_settings",
]
