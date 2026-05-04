import importlib.util
from pathlib import Path
from types import SimpleNamespace

from ixa_epi_covid.phase1 import core


def test_resolve_synth_population_file_uses_env_copy(monkeypatch, tmp_path):
    monkeypatch.chdir(tmp_path)
    env_source = tmp_path / "external.csv"
    env_source.write_text("person_id\n1\n", encoding="utf-8")
    config = SimpleNamespace(
        use_env_synth_pop_file=True,
        state="Indiana",
        year=2020,
        target_data=core.DEFAULT_TARGET_DATA,
        tolerance_values=[2.0],
    )

    resolved = core.resolve_synth_population_file(
        config,
        env={"SYNTH_POP_FILE": str(env_source)},
    )

    assert resolved == Path("experiments/phase1/input/external.csv")
    assert resolved.read_text(encoding="utf-8") == "person_id\n1\n"


def test_resolve_synth_population_file_builds_default_population(
    monkeypatch, tmp_path
):
    monkeypatch.chdir(tmp_path)
    calls: list[list[str]] = []
    config = SimpleNamespace(
        use_env_synth_pop_file=False,
        state="Indiana",
        year=2020,
        target_data=core.DEFAULT_TARGET_DATA,
        tolerance_values=[2.0],
    )

    def fake_create_population(args: list[str]) -> None:
        calls.append(args)

    resolved = core.resolve_synth_population_file(
        config,
        default_population_size_dev="50_000",
        create_population_func=fake_create_population,
    )

    assert resolved == Path("input/synth_pop_people_IN_50_000.csv")
    assert calls == [["--size", "50_000", "--state", "IN", "--year", "2020"]]


def test_resolve_synth_population_file_uses_default_creator_function(
    monkeypatch, tmp_path
):
    monkeypatch.chdir(tmp_path)
    calls: list[list[str]] = []
    config = SimpleNamespace(
        use_env_synth_pop_file=False,
        state="Indiana",
        year=2020,
        target_data=core.DEFAULT_TARGET_DATA,
        tolerance_values=[2.0],
    )

    def fake_create_population(args: list[str]) -> None:
        calls.append(args)

    monkeypatch.setattr(
        core,
        "create_synthetic_population_run",
        fake_create_population,
    )

    resolved = core.resolve_synth_population_file(
        config,
        default_population_size_dev="50_000",
    )

    assert resolved == Path("input/synth_pop_people_IN_50_000.csv")
    assert calls == [["--size", "50_000", "--state", "IN", "--year", "2020"]]


def test_format_synth_population_summary_for_cloud():
    summary = core.format_synth_population_summary(
        "input/synth_pop_people_IN_50_000.csv",
        cloud=True,
    )

    assert "input/synth_pop_people_IN_50_000.csv" in summary
    assert "population size 50_000" in summary
    assert "shared by all cloud simulations" in summary


def test_format_synth_population_summary_for_custom_file():
    summary = core.format_synth_population_summary(
        "experiments/phase1/input/external.csv",
        cloud=False,
    )

    assert "experiments/phase1/input/external.csv" in summary
    assert "population size unknown" in summary
    assert "shared by all simulations" in summary


def test_prepare_output_dir_overwrites_existing(tmp_path):
    output_dir = tmp_path / "output"
    output_dir.mkdir()
    (output_dir / "old.txt").write_text("old", encoding="utf-8")

    resolved = core.prepare_output_dir(output_dir, force_overwrite=True)

    assert resolved == output_dir
    assert resolved.exists()
    assert list(resolved.iterdir()) == []


def test_script_wrapper_exports_packaged_entrypoint():
    script_path = (
        Path(__file__).resolve().parents[2]
        / "scripts"
        / "phase_1_calibration.py"
    )
    spec = importlib.util.spec_from_file_location(
        "phase_1_calibration_script",
        script_path,
    )
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    from ixa_epi_covid.phase1.calibrate import run_phase1_calibration

    assert module.main is run_phase1_calibration


def test_phase1_rows_to_report_casts_csv_table_strings():
    report = core.phase1_rows_to_report(
        {
            "t_lower": ["0.0"],
            "t_upper": ["1.0"],
            "count": ["2"],
        }
    )

    assert report["t_lower"].to_list() == [0.0]
    assert report["t_upper"].to_list() == [1.0]
    assert report["count"].to_list() == [2]
