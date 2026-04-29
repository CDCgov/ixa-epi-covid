from pathlib import Path

import polars as pl
import pytest
from mrp.runtime import RunResult

from ixa_epi_covid.mrp_runner import IxaEpiCovidMRPRunner


def test_mrp_runner_parses_phase1_output(monkeypatch):
    def fake_mrp_run(config_path, overrides):
        assert config_path == Path("/tmp/ixa_epi_covid.mrp.toml")
        assert overrides["input"] == {"seed": 123}
        assert overrides["output"] == {"spec": "stdout"}
        return RunResult(
            exit_code=0,
            stdout=(
                b"Running task...\nt_lower,t_upper,count\n0.0,1.0,0\n1.0,2.0,1\n"
            ),
            stderr=b"",
        )

    runner = IxaEpiCovidMRPRunner(
        "/tmp/ixa_epi_covid.mrp.toml",
        mrp_run_func=fake_mrp_run,
    )

    output = runner.simulate({"seed": 123})

    assert output["aggregated_deaths_report"] == {
        "t_lower": [0.0, 1.0],
        "t_upper": [1.0, 2.0],
        "count": [0, 1],
    }


def test_mrp_runner_raises_on_failed_run():
    def fake_mrp_run(config_path, overrides):
        return RunResult(
            exit_code=1,
            stdout=b"",
            stderr=b"model failed",
        )

    runner = IxaEpiCovidMRPRunner(
        "/tmp/ixa_epi_covid.mrp.toml",
        mrp_run_func=fake_mrp_run,
    )

    with pytest.raises(RuntimeError, match="model failed"):
        runner.simulate({"seed": 123})


def test_mrp_runner_uses_staged_input_and_output_dirs(tmp_path):
    input_path = tmp_path / "input.json"
    input_path.write_text(
        '{"seed": 123, "run_id": "gen_0_particle_0_attempt_0"}',
        encoding="utf-8",
    )
    run_output_dir = tmp_path / "output"

    def fake_mrp_run(config_path, overrides, output_dir=None):
        assert overrides["input"] == str(input_path)
        assert output_dir == str(run_output_dir)
        run_output_dir.mkdir(parents=True, exist_ok=True)
        pl.DataFrame(
            {
                "t_lower": [0.0],
                "t_upper": [1.0],
                "count": [1],
            }
        ).write_csv(run_output_dir / "output.csv")
        return RunResult(exit_code=0, stdout=b"", stderr=b"")

    runner = IxaEpiCovidMRPRunner(
        "/tmp/ixa_epi_covid.mrp.toml",
        mrp_run_func=fake_mrp_run,
    )

    output = runner.simulate(
        {"seed": 123},
        input_path=input_path,
        output_dir=run_output_dir,
        run_id="gen_0_particle_0_attempt_0",
    )

    assert output["aggregated_deaths_report"]["count"] == [1]
