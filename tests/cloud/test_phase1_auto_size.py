import argparse
from types import SimpleNamespace

import pytest

from ixa_epi_covid.phase1 import calibrate


def _args(**overrides):
    values = {
        "auto_size": False,
        "cloud": False,
        "cloud_config": calibrate.DEFAULT_CLOUD_CONFIG_PATH,
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

    def fake_resolve_cloud_sizing_from_config(**kwargs):
        calls.update(kwargs)
        return returned_sizing

    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_sizing_from_config",
        fake_resolve_cloud_sizing_from_config,
    )

    base_inputs = {"seed": 1}
    sizing = calibrate.resolve_cloud_sizing(
        _args(auto_size=True, cloud=True, max_workers=10),
        base_inputs=base_inputs,
    )

    assert sizing is returned_sizing
    assert calls["cloud_config_path"] == calibrate.DEFAULT_CLOUD_CONFIG_PATH
    assert calls["base_inputs"] is base_inputs
    assert calls["auto_size"] is True
    assert calls["cloud"] is True
    assert calls["max_concurrent_simulations"] == 10
    assert calls["max_concurrent_simulations_explicit"] is True


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

    def fake_resolve_cloud_sizing_from_config(**kwargs):
        calls.update(kwargs)
        return SimpleNamespace(
            max_concurrent_simulations=12,
            task_slots_per_node_override=4,
            summary=None,
        )

    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_sizing_from_config",
        fake_resolve_cloud_sizing_from_config,
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

    def fake_resolve_cloud_sizing_from_config(**kwargs):
        calls.update(kwargs)
        return SimpleNamespace(
            max_concurrent_simulations=25,
            task_slots_per_node_override=5,
            summary=None,
        )

    monkeypatch.setattr(
        calibrate,
        "resolve_cloud_sizing_from_config",
        fake_resolve_cloud_sizing_from_config,
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


def test_resolve_model_runner_passes_base_inputs_to_cloud_factory(
    monkeypatch,
):
    calls: dict[str, object] = {}
    sizing = SimpleNamespace(
        max_concurrent_simulations=12,
        task_slots_per_node_override=4,
        summary=object(),
    )

    def fake_cloud_runner_factory(config_path, **kwargs):
        calls["config_path"] = config_path
        calls.update(kwargs)
        return "runner"

    monkeypatch.setattr(
        calibrate,
        "create_cloud_mrp_runner_from_config",
        fake_cloud_runner_factory,
    )

    base_inputs = {"seed": 1}
    runner = calibrate.resolve_model_runner(
        argparse.Namespace(
            cloud=True,
            cloud_config=calibrate.DEFAULT_CLOUD_CONFIG_PATH,
            mrp_config=None,
            docker=False,
            print_task_durations=True,
            max_concurrent_simulations=None,
            max_workers=None,
        ),
        generation_count=2,
        base_inputs=base_inputs,
        cloud_sizing=sizing,
    )

    assert runner == "runner"
    assert calls["config_path"] == calibrate.DEFAULT_CLOUD_CONFIG_PATH
    assert calls["generation_count"] == 2
    assert calls["max_concurrent_simulations"] == 12
    assert calls["output_contract"] is calibrate.PHASE1_OUTPUT_CONTRACT
    assert calls["base_inputs"] is base_inputs
    assert calls["print_task_durations"] is True
    assert calls["task_slots_per_node_override"] == 4
    assert calls["auto_size_summary"] is sizing.summary


def test_resolve_model_runner_defaults_to_direct_runner():
    runner = calibrate.resolve_model_runner(
        argparse.Namespace(
            cloud=False,
            cloud_config=calibrate.DEFAULT_CLOUD_CONFIG_PATH,
            mrp_config=None,
            docker=False,
            print_task_durations=False,
            max_concurrent_simulations=None,
            max_workers=None,
        ),
        generation_count=2,
        base_inputs={"seed": 1},
    )

    assert isinstance(runner, calibrate.IxaEpiCovidDirectRunner)
