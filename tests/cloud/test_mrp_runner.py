import json
import os
import sys
import textwrap
from pathlib import Path

import polars as pl
import pytest
from mrp.runtime import RunResult

from ixa_epi_covid.mrp_runner import (
    DEFAULT_DOCKER_MRP_CONFIG_PATH,
    IxaEpiCovidMRPRunner,
)
from ixa_epi_covid.phase1.calibrate import DOCKER_IXA_EXECUTABLE


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


def test_docker_mrp_runner_forwards_staged_input_to_task_runner(
    monkeypatch,
    tmp_path,
):
    fake_bin_dir = tmp_path / "bin"
    fake_bin_dir.mkdir()
    fake_docker = fake_bin_dir / "docker"
    fake_docker.write_text(
        textwrap.dedent(
            f"""\
            #!{sys.executable}
            import json
            import sys
            from pathlib import Path

            expected_command = [
                "/app/.venv/bin/python",
                "-m",
                "ixa_epi_covid.mrp_task_runner",
            ]
            args = sys.argv[1:]
            try:
                image_index = args.index("ixa-epi-covid-cloud:latest")
            except ValueError:
                print("missing cloud image argument", file=sys.stderr)
                raise SystemExit(2)

            actual_command = args[image_index + 1:]
            if actual_command != expected_command:
                print(
                    f"unexpected container command: {{actual_command!r}}",
                    file=sys.stderr,
                )
                raise SystemExit(3)

            transport = json.loads(sys.stdin.read())
            model_input = transport["input"]
            missing = [
                key
                for key in [
                    "ixa_inputs",
                    "config_inputs",
                    "importation_inputs",
                    "run_id",
                ]
                if key not in model_input
            ]
            if missing:
                print(f"missing staged input keys: {{missing!r}}", file=sys.stderr)
                raise SystemExit(4)

            if model_input["run_id"] != "gen_0_particle_0_attempt_0":
                print("run_id was not forwarded", file=sys.stderr)
                raise SystemExit(5)

            container_exe = model_input["config_inputs"]["exe_file"]
            if container_exe != {DOCKER_IXA_EXECUTABLE!r}:
                print(
                    "container IXA executable was not forwarded",
                    file=sys.stderr,
                )
                raise SystemExit(6)

            output_dir = Path(transport["output"]["dir"])
            output_dir.mkdir(parents=True, exist_ok=True)
            (output_dir / "output.csv").write_text(
                "t_lower,t_upper,count\\n0.0,1.0,2\\n",
                encoding="utf-8",
            )
            """
        ),
        encoding="utf-8",
    )
    fake_docker.chmod(0o755)
    monkeypatch.setenv(
        "PATH",
        f"{fake_bin_dir}{os.pathsep}{os.environ['PATH']}",
    )

    input_path = tmp_path / "input" / "gen_0_particle_0_attempt_0.json"
    input_path.parent.mkdir(parents=True)
    input_path.write_text(
        json.dumps(
            {
                "run_id": "gen_0_particle_0_attempt_0",
                "ixa_inputs": {"epimodel.GlobalParams": {"seed": 123}},
                "config_inputs": {
                    "exe_file": DOCKER_IXA_EXECUTABLE,
                    "output_dir": "original-output",
                    "outputs_to_read": ["aggregated_deaths_report"],
                },
                "importation_inputs": {
                    "state": "Indiana",
                    "year": 2020,
                    "symptomatic_reporting_prob": 0.5,
                },
            }
        ),
        encoding="utf-8",
    )
    run_output_dir = tmp_path / "output" / "gen_0_particle_0_attempt_0"

    runner = IxaEpiCovidMRPRunner(DEFAULT_DOCKER_MRP_CONFIG_PATH)
    output = runner.simulate(
        {"ignored": True},
        input_path=input_path,
        output_dir=run_output_dir,
        run_id="gen_0_particle_0_attempt_0",
    )

    assert output["aggregated_deaths_report"]["count"] == [2]
