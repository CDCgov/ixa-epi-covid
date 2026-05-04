import tomllib
from pathlib import Path

from calibrationtools.cloud.config import (
    CloudAutoSizeMemoryScope,
    CloudOutputMode,
    CSVTableOrientation,
    load_cloud_model_config,
)

REPO_ROOT = Path(__file__).resolve().parents[2]


def _load_config(name: str) -> dict:
    with (REPO_ROOT / name).open("rb") as fp:
        return tomllib.load(fp)


def test_cloud_config_uses_model_facing_schema():
    cloud_config = load_cloud_model_config(
        REPO_ROOT / "ixa_epi_covid.cloud_config.toml"
    )

    assert cloud_config.build_context == REPO_ROOT
    assert cloud_config.dockerfile == REPO_ROOT / "Dockerfile.cloud"
    assert cloud_config.runtime_settings.repository == "ixa-epi-covid-cloud"
    assert cloud_config.runtime_settings.jobs_per_session == 1
    assert cloud_config.runtime_settings.task_mrp_config_path == (
        "/app/ixa_epi_covid.mrp.task.toml"
    )
    assert cloud_config.runtime_settings.max_parallel_output_downloads == 8
    assert cloud_config.output.filename == "output.csv"
    assert cloud_config.output.mode is CloudOutputMode.CSV_TABLE
    assert cloud_config.output.output_name == "aggregated_deaths_report"
    assert cloud_config.output.orientation is CSVTableOrientation.COLUMNS
    assert cloud_config.shared_assets[0].name == "synthetic_population"
    assert cloud_config.auto_size.probe == "local_task"
    assert cloud_config.auto_size.local_mrp_config_path == (
        REPO_ROOT / "ixa_epi_covid.mrp.toml"
    )
    assert (
        cloud_config.auto_size.memory_scope
        is CloudAutoSizeMemoryScope.PROCESS_TREE
    )


def test_local_task_and_docker_mrp_configs_remain_process_backed():
    expected_commands = {
        "ixa_epi_covid.mrp.toml": "python",
        "ixa_epi_covid.mrp.task.toml": "/app/.venv/bin/python",
        "ixa_epi_covid.mrp.docker.toml": "sh",
    }

    for config_name, command in expected_commands.items():
        config = _load_config(config_name)
        assert config["runtime"]["spec"] == "process"
        assert config["runtime"]["command"] == command
        assert "callable" not in config["runtime"]
