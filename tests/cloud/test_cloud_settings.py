from pathlib import Path

import pytest
from calibrationtools.cloud.config import (
    CloudAutoSizeMemoryScope,
    CloudOutputMode,
    CSVTableOrientation,
    DEFAULT_INPUT_MOUNT_PATH,
    load_cloud_model_config,
)


def _write_cloud_config(
    tmp_path: Path,
    *,
    jobs_key: str = "jobs_per_session",
    jobs_value: int = 3,
) -> Path:
    (tmp_path / "Dockerfile.cloud").write_text(
        "FROM scratch\n",
        encoding="utf-8",
    )
    config_path = tmp_path / "cloud_config.toml"
    config_path.write_text(
        f"""
[cloud]
keyvault = "cfa-predict"
vm_size = "large"
{jobs_key} = {jobs_value}
task_slots_per_node = 50
pool_max_nodes = 5
dispatch_buffer = 1000

[cloud.image]
local_image = "ixa-epi-covid-cloud"
repository = "custom-repository"
build_context = "."
dockerfile = "Dockerfile.cloud"
task_mrp_config_path = "/app/ixa_epi_covid.mrp.task.toml"

[cloud.resources]
pool_prefix = "ixa-epi-covid-cloud"
job_prefix = "ixa-epi-covid-cloud"
input_container_prefix = "ixa-epi-covid-cloud-input"
output_container_prefix = "ixa-epi-covid-cloud-output"
logs_container_prefix = "ixa-epi-covid-cloud-logs"

[cloud.output]
filename = "output.csv"
csv_value_column = "count"
csv_value_type = "int"
""",
        encoding="utf-8",
    )
    return config_path


def test_load_cloud_model_config_uses_model_facing_runtime_settings(tmp_path):
    config_path = _write_cloud_config(tmp_path)

    settings = load_cloud_model_config(config_path).runtime_settings

    assert settings.keyvault == "cfa-predict"
    assert settings.input_mount_path == DEFAULT_INPUT_MOUNT_PATH
    assert settings.repository == "custom-repository"
    assert settings.jobs_per_session == 3


def test_load_cloud_model_config_accepts_legacy_jobs_per_generation(
    tmp_path,
):
    config_path = _write_cloud_config(
        tmp_path,
        jobs_key="jobs_per_generation",
        jobs_value=2,
    )

    with pytest.deprecated_call():
        settings = load_cloud_model_config(config_path).runtime_settings

    assert settings.jobs_per_session == 2


def test_packaged_cloud_config_loads():
    config_path = (
        Path(__file__).resolve().parents[2] / "ixa_epi_covid.cloud_config.toml"
    )

    config = load_cloud_model_config(config_path)

    assert config.runtime_settings.repository == "ixa-epi-covid-cloud"
    assert config.runtime_settings.jobs_per_session == 1
    assert config.runtime_settings.max_parallel_output_downloads == 8
    assert config.output.filename == "output.csv"
    assert config.output.mode is CloudOutputMode.CSV_TABLE
    assert config.output.output_name == "aggregated_deaths_report"
    assert config.output.orientation is CSVTableOrientation.COLUMNS
    assert config.shared_assets[0].name == "synthetic_population"
    assert config.auto_size.probe == "local_task"
    assert config.auto_size.local_mrp_config_path == (
        Path(__file__).resolve().parents[2] / "ixa_epi_covid.mrp.toml"
    )
    assert (
        config.auto_size.memory_scope
        is CloudAutoSizeMemoryScope.PROCESS_TREE
    )
