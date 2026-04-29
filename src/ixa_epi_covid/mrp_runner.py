from __future__ import annotations

import io
from pathlib import Path
from typing import Any

import polars as pl
from calibrationtools.json_utils import to_jsonable
from calibrationtools.mrp_csv_runner import extract_csv_from_output_text
from mrp import run as mrp_run

from .model_execution import (
    CANONICAL_OUTPUT_FILENAME,
    PHASE1_OUTPUT_NAME,
    phase1_report_to_rows,
)

_REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MRP_CONFIG_PATH = _REPO_ROOT / "ixa_epi_covid.mrp.toml"
DEFAULT_DOCKER_MRP_CONFIG_PATH = _REPO_ROOT / "ixa_epi_covid.mrp.docker.toml"
DEFAULT_CLOUD_MRP_CONFIG_PATH = _REPO_ROOT / "ixa_epi_covid.mrp.cloud.toml"
_OUTPUT_HEADER_FIELDS = ("t_lower", "t_upper", "count")


def read_phase1_output_dir(
    output_dir: Path,
) -> dict[str, dict[str, list[Any]]]:
    output_path = Path(output_dir) / CANONICAL_OUTPUT_FILENAME
    if not output_path.exists():
        raise FileNotFoundError(
            f"MRP model did not write expected output file: {output_path}"
        )
    return {
        PHASE1_OUTPUT_NAME: phase1_report_to_rows(pl.read_csv(output_path))
    }


class IxaEpiCovidMRPRunner:
    """Thin MRP runner for the phase-1 single-report task contract."""

    def __init__(
        self,
        config_path: str | Path = DEFAULT_MRP_CONFIG_PATH,
        *,
        mrp_run_func=None,
    ) -> None:
        self.config_path = Path(config_path)
        self._mrp_run = mrp_run if mrp_run_func is None else mrp_run_func

    def read_output_dir(
        self,
        output_dir: str | Path,
    ) -> dict[str, dict[str, list[Any]]]:
        return read_phase1_output_dir(Path(output_dir))

    def simulate(
        self,
        params: dict[str, Any],
        *,
        input_path: str | Path | None = None,
        output_dir: str | Path | None = None,
        run_id: str | None = None,
    ) -> dict[str, dict[str, list[Any]]]:
        if input_path is not None:
            overrides: dict[str, Any] = {"input": str(input_path)}
            if output_dir is None:
                overrides["output"] = {"spec": "stdout"}
        else:
            overrides = {
                "input": to_jsonable(params),
                "output": {"spec": "stdout"},
            }

        run_kwargs: dict[str, Any] = {}
        if output_dir is not None:
            run_kwargs["output_dir"] = str(output_dir)

        result = self._mrp_run(
            self.config_path,
            overrides,
            **run_kwargs,
        )
        if not result.ok:
            prefix = f"run {run_id}: " if run_id else ""
            raise RuntimeError(prefix + result.stderr.decode())

        if output_dir is not None:
            return self.read_output_dir(output_dir)

        csv_text = extract_csv_from_output_text(
            result.stdout.decode(),
            header_fields=_OUTPUT_HEADER_FIELDS,
        )
        return {
            PHASE1_OUTPUT_NAME: phase1_report_to_rows(
                pl.read_csv(io.StringIO(csv_text))
            )
        }
