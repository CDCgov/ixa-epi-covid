import argparse
import importlib.util
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "phase_1_calibration",
    REPO_ROOT / "scripts/phase_1_calibration.py",
)
assert SPEC is not None
assert SPEC.loader is not None
phase_1_calibration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(phase_1_calibration)

DEFAULT_ARTIFACTS_DIR = phase_1_calibration.DEFAULT_ARTIFACTS_DIR
DOCKER_IXA_EXECUTABLE = phase_1_calibration.DOCKER_IXA_EXECUTABLE
apply_local_docker_runtime_overrides = (
    phase_1_calibration.apply_local_docker_runtime_overrides
)
main = phase_1_calibration.main
resolve_artifacts_dir = phase_1_calibration.resolve_artifacts_dir


def _args(**overrides):
    defaults = {
        "artifacts_dir": None,
        "auto_size": False,
        "cloud": False,
        "cloud_config": Path("ixa_epi_covid.cloud_config.toml"),
        "config_file": Path("unused.yaml"),
        "default_population_size_dev": "1_000",
        "docker": False,
        "max_concurrent_simulations": None,
        "max_workers": None,
        "mrp_config": None,
        "no_artifacts": False,
        "output_dir": Path("unused-output"),
        "print_task_durations": False,
        "print_task_progress": False,
    }
    defaults.update(overrides)
    return argparse.Namespace(**defaults)


def test_cloud_no_artifacts_fails_before_runner_creation():
    with pytest.raises(ValueError, match="cloud requires artifacts"):
        resolve_artifacts_dir(_args(cloud=True, no_artifacts=True))


def test_docker_mode_defaults_to_artifact_staging():
    assert resolve_artifacts_dir(_args(docker=True)) == DEFAULT_ARTIFACTS_DIR


def test_auto_size_requires_cloud_before_config_load():
    with pytest.raises(ValueError, match="--auto-size requires --cloud"):
        main("unused.yaml", "unused-output", auto_size=True)


def test_docker_runner_selection_applies_container_executable_override():
    model_inputs = {
        "config_inputs": {"exe_file": "./target/release/ixa-epi-covid"}
    }

    resolved = apply_local_docker_runtime_overrides(
        model_inputs,
        docker=True,
        cloud=False,
        mrp_config=None,
    )

    assert resolved["config_inputs"]["exe_file"] == DOCKER_IXA_EXECUTABLE
    assert model_inputs["config_inputs"]["exe_file"] == (
        "./target/release/ixa-epi-covid"
    )
