import json
from pathlib import Path
from typing import Any

import polars as pl
from mrp import MRPModel

from .config_parser import update_epimodel_output_dir
from .model_execution import (
    CANONICAL_OUTPUT_FILENAME,
    PHASE1_OUTPUT_NAME,
    execute_phase1_model,
    phase1_report_to_rows,
    write_canonical_output_csv,
)


class CovidModel(MRPModel):
    def run(self):
        outputs = self.simulate(self.input)
        self.write_csv(
            CANONICAL_OUTPUT_FILENAME,
            phase1_report_to_rows(outputs[PHASE1_OUTPUT_NAME]),
        )

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> dict[str, pl.DataFrame]:
        return execute_phase1_model(model_inputs)


class IxaEpiCovidDirectRunner:
    """Run the phase-1 model locally while honoring staged sampler I/O."""

    def simulate(
        self,
        model_inputs: dict[str, Any],
        *,
        input_path: str | Path | None = None,
        output_dir: str | Path | None = None,
        run_id: str | None = None,
    ) -> dict[str, dict[str, list[Any]]]:
        resolved_inputs = self._resolve_inputs(
            model_inputs,
            input_path=input_path,
            output_dir=output_dir,
            run_id=run_id,
        )
        outputs = execute_phase1_model(resolved_inputs)
        if output_dir is not None:
            write_canonical_output_csv(
                Path(output_dir) / CANONICAL_OUTPUT_FILENAME,
                outputs,
            )
        return {
            output_name: phase1_report_to_rows(report)
            for output_name, report in outputs.items()
        }

    @staticmethod
    def _resolve_inputs(
        model_inputs: dict[str, Any],
        *,
        input_path: str | Path | None,
        output_dir: str | Path | None,
        run_id: str | None,
    ) -> dict[str, Any]:
        if input_path is None:
            resolved_inputs = dict(model_inputs)
        else:
            loaded_inputs = json.loads(Path(input_path).read_text())
            if not isinstance(loaded_inputs, dict):
                raise ValueError("IXA phase-1 input JSON must be an object.")
            resolved_inputs = loaded_inputs

        if output_dir is not None:
            resolved_inputs = update_epimodel_output_dir(
                resolved_inputs,
                output_dir,
            )
        if run_id is not None:
            resolved_inputs["run_id"] = run_id
        return resolved_inputs


def main() -> int:
    CovidModel().run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
