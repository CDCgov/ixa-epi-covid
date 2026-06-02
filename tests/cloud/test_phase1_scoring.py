import importlib.util
from pathlib import Path

import polars as pl

from ixa_epi_covid.covid_model import PHASE1_OUTPUT_NAME

REPO_ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "phase_1_calibration",
    REPO_ROOT / "scripts/phase_1_calibration.py",
)
assert SPEC is not None
assert SPEC.loader is not None
phase_1_calibration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(phase_1_calibration)

TARGET_DATA = phase_1_calibration.TARGET_DATA
outputs_to_distance = phase_1_calibration.outputs_to_distance


def test_outputs_to_distance_accepts_polars_dataframe():
    output = {
        PHASE1_OUTPUT_NAME: pl.DataFrame(
            {"t_lower": [70], "t_upper": [74], "count": [1]}
        )
    }

    assert outputs_to_distance(output, TARGET_DATA) == 1


def test_outputs_to_distance_accepts_csv_table_column_mapping():
    output = {
        PHASE1_OUTPUT_NAME: {
            "t_lower": ["70"],
            "t_upper": ["74"],
            "count": ["1"],
        }
    }

    assert outputs_to_distance(output, TARGET_DATA) == 1
