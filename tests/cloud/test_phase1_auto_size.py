import argparse
from types import SimpleNamespace

import pytest

from ixa_epi_covid.phase1 import calibrate


def _args(**overrides):
    values = {
        "auto_size": False,
        "cloud": False,
        "artifacts_dir": None,
        "no_artifacts": False,
        "max_concurrent_simulations": None,
        "max_workers": None,
    }
    values.update(overrides)
    return argparse.Namespace(**values)


def test_resolve_cloud_sizing_rejects_auto_size_without_cloud():
    with pytest.raises(ValueError, match="--auto-size requires --cloud"):
        calibrate.resolve_cloud_sizing(
            _args(auto_size=True, cloud=False),
            base_inputs={"seed": 1},
        )


def test_resolve_cloud_sizing_uses_cloud_config_and_probe(monkeypatch):
    calls: dict[str, object] = {}
    returned_sizing = SimpleNamespace(
        max_concurrent_simulations=14,
        task_slots_per_node_override=7,
        summary=object(),
    )

    monkeypatch.setattr(
        calibrate,
        "load_cloud_runtime_settings",
        lambda config_path: SimpleNamespace(
            vm_size="large",
            pool_max_nodes=2,
        ),
    )

    def fake_probe(module_name, base_inputs):
        calls["probe_module"] = module_name
        calls["probe_inputs"] = base_inputs
        return 123_456

    def fake_resolve_cloud_auto_size(**kwargs):
        calls["auto_size_kwargs"] = kwargs
        assert kwargs["measure_task_peak_rss_bytes"]() == 123_456
        return returned_sizing

    monkeypatch.setattr(calibrate, "run_local_memory_probe", fake_probe)
    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_auto_size",
        fake_resolve_cloud_auto_size,
    )

    base_inputs = {"seed": 1}
    sizing = calibrate.resolve_cloud_sizing(
        _args(auto_size=True, cloud=True, max_workers=10),
        base_inputs=base_inputs,
    )

    assert sizing is returned_sizing
    assert calls["probe_module"] == "ixa_epi_covid.cloud.auto_size"
    assert calls["probe_inputs"] is base_inputs
    assert calls["auto_size_kwargs"]["vm_size"] == "large"
    assert calls["auto_size_kwargs"]["pool_max_nodes"] == 2
    assert calls["auto_size_kwargs"]["max_concurrent_simulations"] == 10
    assert (
        calls["auto_size_kwargs"]["max_concurrent_simulations_explicit"]
        is True
    )


def test_resolve_artifacts_dir_defaults_to_shared_artifacts_dir():
    assert (
        calibrate.resolve_artifacts_dir(_args())
        == calibrate.DEFAULT_ARTIFACTS_DIR
    )


def test_resolve_artifacts_dir_allows_explicit_disable_for_local_runs():
    assert calibrate.resolve_artifacts_dir(_args(no_artifacts=True)) is None


def test_resolve_artifacts_dir_rejects_no_artifacts_for_cloud():
    with pytest.raises(ValueError, match="--cloud requires artifacts"):
        calibrate.resolve_artifacts_dir(
            _args(cloud=True, no_artifacts=True),
        )


def test_resolve_artifacts_dir_rejects_conflicting_flags(tmp_path):
    with pytest.raises(
        ValueError, match="either --artifacts-dir or --no-artifacts"
    ):
        calibrate.resolve_artifacts_dir(
            _args(artifacts_dir=tmp_path, no_artifacts=True),
        )


def test_resolve_cloud_sizing_preserves_explicit_concurrency(monkeypatch):
    calls: dict[str, object] = {}

    monkeypatch.setattr(
        calibrate,
        "load_cloud_runtime_settings",
        lambda config_path: SimpleNamespace(
            vm_size="large",
            pool_max_nodes=5,
        ),
    )
    monkeypatch.setattr(
        calibrate,
        "run_local_memory_probe",
        lambda module_name, base_inputs: 123_456,
    )

    def fake_resolve_cloud_auto_size(**kwargs):
        calls.update(kwargs)
        return SimpleNamespace(
            max_concurrent_simulations=12,
            task_slots_per_node_override=4,
            summary=None,
        )

    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_auto_size",
        fake_resolve_cloud_auto_size,
    )

    calibrate.resolve_cloud_sizing(
        _args(
            auto_size=True,
            cloud=True,
            max_concurrent_simulations=12,
            max_workers=99,
        ),
        base_inputs={"seed": 1},
    )

    assert calls["max_concurrent_simulations"] == 12
    assert calls["max_concurrent_simulations_explicit"] is True


def test_resolve_cloud_sizing_uses_default_as_implicit_concurrency(
    monkeypatch,
):
    calls: dict[str, object] = {}

    monkeypatch.setattr(
        calibrate,
        "load_cloud_runtime_settings",
        lambda config_path: SimpleNamespace(
            vm_size="large",
            pool_max_nodes=5,
        ),
    )
    monkeypatch.setattr(
        calibrate,
        "run_local_memory_probe",
        lambda module_name, base_inputs: 123_456,
    )

    def fake_resolve_cloud_auto_size(**kwargs):
        calls.update(kwargs)
        return SimpleNamespace(
            max_concurrent_simulations=25,
            task_slots_per_node_override=5,
            summary=None,
        )

    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_auto_size",
        fake_resolve_cloud_auto_size,
    )

    calibrate.resolve_cloud_sizing(
        _args(auto_size=True, cloud=True),
        base_inputs={"seed": 1},
    )

    assert (
        calls["max_concurrent_simulations"]
        == calibrate.DEFAULT_MAX_CONCURRENT_SIMULATIONS
    )
    assert calls["max_concurrent_simulations_explicit"] is False


def test_resolve_model_runner_passes_auto_size_to_cloud_runner(monkeypatch):
    calls: dict[str, object] = {}
    sizing = SimpleNamespace(
        max_concurrent_simulations=12,
        task_slots_per_node_override=4,
        summary=object(),
    )

    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_build_context",
        lambda repo_root=None, dockerfile=None: ("repo", "dockerfile"),
    )

    def fake_cloud_runner(config_path, **kwargs):
        calls["config_path"] = config_path
        calls.update(kwargs)
        return "runner"

    monkeypatch.setattr(
        calibrate,
        "IxaEpiCovidCloudRunner",
        fake_cloud_runner,
    )

    runner = calibrate.resolve_model_runner(
        argparse.Namespace(
            cloud=True,
            mrp_config=None,
            docker=False,
            repo_root=None,
            dockerfile=None,
            print_task_durations=True,
            max_concurrent_simulations=None,
            max_workers=None,
        ),
        generation_count=2,
        synth_population_file="population.csv",
        cloud_sizing=sizing,
    )

    assert runner == "runner"
    assert calls["config_path"] == calibrate.DEFAULT_CLOUD_MRP_CONFIG_PATH
    assert calls["max_concurrent_simulations"] == 12
    assert calls["task_slots_per_node_override"] == 4
    assert calls["auto_size_summary"] is sizing.summary


def test_resolve_model_runner_defaults_to_direct_runner():
    runner = calibrate.resolve_model_runner(
        argparse.Namespace(
            cloud=False,
            mrp_config=None,
            docker=False,
            repo_root=None,
            dockerfile=None,
            print_task_durations=False,
            max_concurrent_simulations=None,
            max_workers=None,
        ),
        generation_count=2,
        synth_population_file="population.csv",
    )

    assert isinstance(runner, calibrate.IxaEpiCovidDirectRunner)
