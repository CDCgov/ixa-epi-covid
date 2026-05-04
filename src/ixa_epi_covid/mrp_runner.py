from __future__ import annotations

import io
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import polars as pl
from calibrationtools.mrp_csv_runner import (
    MRPOutputRunner,
    extract_csv_from_output_text,
)
from calibrationtools.output_contracts import OutputContract
from mrp import run as mrp_run

from .model_execution import (
    CANONICAL_OUTPUT_FILENAME,
    PHASE1_OUTPUT_NAME,
    phase1_report_to_rows,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MRP_CONFIG_PATH = _REPO_ROOT / "ixa_epi_covid.mrp.toml"
DEFAULT_DOCKER_MRP_CONFIG_PATH = _REPO_ROOT / "ixa_epi_covid.mrp.docker.toml"
DEFAULT_CLOUD_CONFIG_PATH = _REPO_ROOT / "ixa_epi_covid.cloud_config.toml"
# Backward-compatible alias for callers that imported the old name before
# calibrationtools moved cloud settings out of the MRP controller config.
DEFAULT_CLOUD_MRP_CONFIG_PATH = DEFAULT_CLOUD_CONFIG_PATH
_OUTPUT_HEADER_FIELDS = ("t_lower", "t_upper", "count")
Phase1Output = dict[str, dict[str, list[Any]]]


def read_phase1_output_dir(
    output_dir: Path,
) -> Phase1Output:
    output_path = Path(output_dir) / CANONICAL_OUTPUT_FILENAME
    if not output_path.exists():
        raise FileNotFoundError(
            f"MRP model did not write expected output file: {output_path}"
        )
    return {
        PHASE1_OUTPUT_NAME: phase1_report_to_rows(pl.read_csv(output_path))
    }


@dataclass(frozen=True)
class Phase1OutputContract(OutputContract[Phase1Output]):
    """Parse the canonical phase-1 report from MRP stdout or output files."""

    @property
    def output_filename(self) -> str:
        return CANONICAL_OUTPUT_FILENAME

    def read_output_dir(self, output_dir: Path) -> Phase1Output:
        return read_phase1_output_dir(output_dir)

    def read_stdout(self, stdout: str | bytes) -> Phase1Output:
        stdout_text = stdout.decode() if isinstance(stdout, bytes) else stdout
        csv_text = extract_csv_from_output_text(
            stdout_text,
            header_fields=_OUTPUT_HEADER_FIELDS,
        )
        return {
            PHASE1_OUTPUT_NAME: phase1_report_to_rows(
                pl.read_csv(io.StringIO(csv_text))
            )
        }


PHASE1_OUTPUT_CONTRACT = Phase1OutputContract()


class IxaEpiCovidMRPRunner(MRPOutputRunner[Phase1Output]):
    """Compatibility wrapper for the phase-1 single-report MRP contract."""

    def __init__(
        self,
        config_path: str | Path = DEFAULT_MRP_CONFIG_PATH,
        *,
        mrp_run_func=None,
    ) -> None:
        super().__init__(
            config_path,
            output_contract=PHASE1_OUTPUT_CONTRACT,
            mrp_run_func=mrp_run if mrp_run_func is None else mrp_run_func,
        )
