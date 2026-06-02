import tomllib
from dataclasses import replace
from pathlib import Path

from calibrationtools.cloud.config import load_cloud_model_config
from calibrationtools.cloud.task_payload import (
    CloudTaskContext,
    apply_task_payload_transforms,
    bind_shared_assets_to_session,
    resolve_shared_assets,
    resolve_task_output_dir,
)

REPO_ROOT = Path(__file__).resolve().parents[2]


def _base_inputs(synth_population: Path) -> dict:
    return {
        "config_inputs": {"output_dir": "/tmp/local-output"},
        "ixa_inputs": {
            "epimodel.GlobalParams": {
                "synth_population_file": str(synth_population),
                "imported_cases_timeseries": {
                    "filename": "/tmp/local-output/imported_cases_timeseries.csv"
                },
            }
        },
    }


def test_cloud_config_loads():
    config = load_cloud_model_config(
        REPO_ROOT / "ixa_epi_covid.cloud_config.toml"
    )

    assert config.output.filename == "output.csv"
    assert config.runtime_settings.repository == "ixa-epi-covid-cloud"
    assert config.runtime_settings.task_mrp_config_path == (
        "/app/ixa_epi_covid.mrp.cloud.toml"
    )


def test_cloud_mrp_config_uses_container_venv_python():
    with open(REPO_ROOT / "ixa_epi_covid.mrp.cloud.toml", "rb") as file:
        config = tomllib.load(file)

    assert config["runtime"]["command"] == "/app/.venv/bin/python"


def test_shared_population_asset_resolves_from_base_inputs(tmp_path):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    cloud_config = load_cloud_model_config(
        REPO_ROOT / "ixa_epi_covid.cloud_config.toml"
    )

    assets = resolve_shared_assets(
        cloud_config.shared_assets,
        base_payload=_base_inputs(synth_population),
        config_dir=cloud_config.config_path.parent.resolve(),
    )

    assert len(assets) == 1
    assert assets[0].name == "synthetic_population"
    assert assets[0].source_path == synth_population.resolve()
    assert assets[0].remote_blob_dir == "shared/synthetic_population"
    assert assets[0].remote_path_var == "SYNTH_POPULATION_PATH"


def test_task_payload_transforms_set_cloud_task_paths(tmp_path):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    cloud_config = load_cloud_model_config(
        REPO_ROOT / "ixa_epi_covid.cloud_config.toml"
    )
    assets = resolve_shared_assets(
        cloud_config.shared_assets,
        base_payload=_base_inputs(synth_population),
        config_dir=cloud_config.config_path.parent.resolve(),
    )
    bound_assets = bind_shared_assets_to_session(
        assets,
        session_id="session-1",
        input_mount_path="/cloud-input",
    )
    base_context = CloudTaskContext(
        run_id="gen_0_particle_0_attempt_0",
        session_id="session-1",
        job_name="generation-0",
        input_mount_path="/cloud-input",
        output_mount_path="/cloud-output",
        logs_mount_path="/cloud-logs",
        task_output_dir="/cloud-output/gen_0_particle_0_attempt_0",
        shared_assets=bound_assets,
    )
    context = replace(
        base_context,
        task_output_dir=resolve_task_output_dir(
            cloud_config.task_payload,
            base_context,
            default_task_output_dir=base_context.task_output_dir,
        ),
    )

    payload = apply_task_payload_transforms(
        _base_inputs(synth_population),
        cloud_config.task_payload,
        context,
    )

    assert (
        payload["config_inputs"]["output_dir"]
        == "/cloud-output/gen_0_particle_0_attempt_0"
    )
    assert (
        payload["ixa_inputs"]["epimodel.GlobalParams"]["synth_population_file"]
        == "/cloud-input/shared/synthetic_population/session-1/synth.csv"
    )
    assert (
        payload["ixa_inputs"]["epimodel.GlobalParams"][
            "imported_cases_timeseries"
        ]["filename"]
        == "/cloud-output/gen_0_particle_0_attempt_0/imported_cases_timeseries.csv"
    )
