from pathlib import Path

from calibrationtools.cloud import auto_size
from calibrationtools.cloud.config import CloudAutoSizeMemoryScope

REPO_ROOT = Path(__file__).resolve().parents[2]


def test_auto_size_uses_configured_local_task_probe(monkeypatch, tmp_path):
    synth_population = tmp_path / "synth.csv"
    synth_population.write_text("person_id\n1\n", encoding="utf-8")
    base_inputs = {
        "config_inputs": {
            "output_dir": str(tmp_path / "output"),
        },
        "ixa_inputs": {
            "epimodel.GlobalParams": {
                "synth_population_file": str(synth_population),
                "imported_cases_timeseries": {
                    "filename": str(tmp_path / "imported_cases_timeseries.csv"),
                },
            }
        },
    }
    calls: dict[str, object] = {}

    def fake_run_local_task_memory_probe(
        cloud_config_path,
        local_mrp_config_path,
        probe_inputs,
        *,
        memory_scope,
    ):
        calls["cloud_config_path"] = cloud_config_path
        calls["local_mrp_config_path"] = local_mrp_config_path
        calls["probe_inputs"] = probe_inputs
        calls["memory_scope"] = memory_scope
        return 512 * 1024 * 1024

    monkeypatch.setattr(
        auto_size,
        "run_local_task_memory_probe",
        fake_run_local_task_memory_probe,
    )

    sizing = auto_size.resolve_cloud_sizing_from_config(
        cloud_config_path=REPO_ROOT / "ixa_epi_covid.cloud_config.toml",
        base_inputs=base_inputs,
        auto_size=True,
        cloud=True,
        max_concurrent_simulations=10,
        max_concurrent_simulations_explicit=True,
    )

    assert calls["cloud_config_path"] == (
        REPO_ROOT / "ixa_epi_covid.cloud_config.toml"
    )
    assert calls["local_mrp_config_path"] == (
        REPO_ROOT / "ixa_epi_covid.mrp.toml"
    )
    assert calls["probe_inputs"] is base_inputs
    assert calls["memory_scope"] is CloudAutoSizeMemoryScope.PROCESS_TREE
    assert sizing.max_concurrent_simulations == 10
    assert sizing.task_slots_per_node_override is not None
