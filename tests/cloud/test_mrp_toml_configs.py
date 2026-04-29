import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def _load_config(name: str) -> dict:
    with (REPO_ROOT / name).open("rb") as fp:
        return tomllib.load(fp)


def test_only_cloud_controller_mrp_config_uses_inline_runtime():
    cloud_config = _load_config("ixa_epi_covid.mrp.cloud.toml")

    assert cloud_config["runtime"]["spec"] == "inline"
    assert (
        cloud_config["runtime"]["callable"]
        == "ixa_epi_covid.cloud.mrp_executor:execute_cloud_run"
    )
    assert "command" not in cloud_config["runtime"]
    assert "args" not in cloud_config["runtime"]


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
