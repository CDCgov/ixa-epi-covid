import importlib.util
from pathlib import Path

import polars as pl
from calibrationtools import MRPOutputRunner
from mrp import Environment

from ixa_epi_covid.covid_model import (
    CANONICAL_OUTPUT_FILENAME,
    PHASE1_OUTPUT_NAME,
    CovidModel,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "phase_1_calibration",
    REPO_ROOT / "scripts/phase_1_calibration.py",
)
assert SPEC is not None
assert SPEC.loader is not None
phase_1_calibration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(phase_1_calibration)

build_phase1_output_contract = phase_1_calibration.build_phase1_output_contract


def test_covid_model_run_writes_canonical_output_csv(monkeypatch, tmp_path):
    def fake_simulate(model_inputs):
        assert model_inputs == {"input": "payload"}
        return {
            PHASE1_OUTPUT_NAME: pl.DataFrame(
                {"t_lower": [0], "t_upper": [1], "count": [2]}
            )
        }

    monkeypatch.setattr(CovidModel, "simulate", staticmethod(fake_simulate))

    model = CovidModel(
        env=Environment(
            {
                "input": {"input": "payload"},
                "output": {"spec": "filesystem", "dir": str(tmp_path)},
            }
        ),
    )
    model.run()

    output = pl.read_csv(tmp_path / CANONICAL_OUTPUT_FILENAME)

    assert output.to_dict(as_series=False) == {
        "t_lower": [0],
        "t_upper": [1],
        "count": [2],
    }


def test_local_mrp_output_runner_parses_fake_output_csv(tmp_path):
    class Result:
        ok = True
        stdout = b""
        stderr = b""

    def fake_mrp_run(config_path, overrides, **kwargs):
        assert Path(config_path) == tmp_path / "model.mrp.toml"
        assert overrides["input"] == {"example": True}
        output_dir = Path(kwargs["output_dir"])
        output_dir.mkdir(parents=True, exist_ok=True)
        (output_dir / CANONICAL_OUTPUT_FILENAME).write_text(
            "t_lower,t_upper,count\n0,1,2\n",
            encoding="utf-8",
        )
        return Result()

    config_path = tmp_path / "model.mrp.toml"
    config_path.write_text("", encoding="utf-8")
    runner = MRPOutputRunner(
        config_path,
        output_contract=build_phase1_output_contract(),
        mrp_run_func=fake_mrp_run,
    )

    output = runner.simulate(
        {"example": True},
        output_dir=tmp_path / "output",
    )

    assert output == {
        PHASE1_OUTPUT_NAME: {
            "t_lower": ["0"],
            "t_upper": ["1"],
            "count": ["2"],
        }
    }
