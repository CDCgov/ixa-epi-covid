from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any

import polars as pl
from importation import ImportationModel, get_linelist_data

PHASE1_OUTPUT_NAME = "aggregated_deaths_report"
CANONICAL_OUTPUT_FILENAME = "output.csv"


def compute_case_fatality_ratio(
    global_params: dict[str, Any],
) -> float:
    """Compute the importation-model case fatality ratio from IXA inputs."""
    n_age_groups = len(global_params["symptom_age_groups"])
    return (
        sum(global_params["probability_severe_given_mild"].values())
        / n_age_groups
        * sum(global_params["probability_critical_given_severe"].values())
        / n_age_groups
        * sum(global_params["probability_dead_given_critical"].values())
        / n_age_groups
    )


def write_input_json(
    ixa_inputs: dict[str, Any],
    *,
    output_dir: Path,
) -> Path:
    """Persist particle-specific IXA inputs so failed runs remain reproducible."""
    output_dir.mkdir(parents=True, exist_ok=True)
    input_file_path = output_dir / "input.json"
    ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
        ixa_inputs["epimodel.GlobalParams"]["seed"]
    )
    input_file_path.write_text(
        json.dumps(ixa_inputs, indent=4),
        encoding="utf-8",
    )
    return input_file_path


def generate_importation_timeseries(
    model_inputs: dict[str, Any],
) -> Path:
    """Generate the importation time series expected by the Rust model."""
    ixa_inputs = model_inputs["ixa_inputs"]
    importation_inputs = model_inputs["importation_inputs"]
    global_params = ixa_inputs["epimodel.GlobalParams"]

    importation_params = {
        "symptomatic_reporting_prob": importation_inputs[
            "symptomatic_reporting_prob"
        ],
        "case_fatality_ratio": compute_case_fatality_ratio(global_params),
        "proportion_asymptomatic": global_params[
            "probability_mild_given_infect"
        ],
    }

    importation_filename = Path(
        global_params["imported_cases_timeseries"]["filename"]
    )
    importation_filename.parent.mkdir(parents=True, exist_ok=True)

    importation_model = ImportationModel(
        data=get_linelist_data(),
        parameters=importation_params,
        national_model="multinomial",
        state_model="proportional",
        seed=global_params["seed"],
    )
    timeseries_data = importation_model.sample_state_importation_incidence(
        proportion=importation_inputs.get("population_proportion"),
        state=importation_inputs["state"],
        year=importation_inputs.get("year"),
    )
    timeseries_data.write_csv(importation_filename)
    return importation_filename


def run_ixa_model(
    *,
    config_inputs: dict[str, Any],
    input_file_path: Path,
) -> None:
    """Execute the compiled IXA transmission model."""
    cmd = [
        config_inputs["exe_file"],
        "--config",
        str(input_file_path),
        "--output",
        config_inputs["output_dir"],
        "--force-overwrite",
        "--no-stats",
    ]

    try:
        subprocess.run(cmd, capture_output=True, check=True, text=True)
    except subprocess.CalledProcessError as exc:
        stderr = (exc.stderr or "").strip()
        stdout = (exc.stdout or "").strip()
        detail = stderr or stdout or "no output captured"
        raise RuntimeError(
            "Error running the ixa model with command "
            f"{' '.join(cmd)}: {detail}"
        ) from exc


def _resolve_output_path(
    *,
    output_dir: Path,
    filename: str,
) -> Path:
    output_path = output_dir / filename
    if output_path.exists():
        return output_path

    filename_path = Path(filename)
    if filename_path.exists():
        return filename_path

    raise FileNotFoundError(
        f"Expected output file {filename} not found. "
        f"Looked in {output_dir} and at {filename_path}."
    )


def read_model_outputs(
    model_inputs: dict[str, Any],
) -> dict[str, pl.DataFrame]:
    """Read configured report CSVs from disk into Polars DataFrames."""
    ixa_inputs = model_inputs["ixa_inputs"]
    config_inputs = model_inputs["config_inputs"]
    output_dir = Path(config_inputs["output_dir"])

    outputs: dict[str, pl.DataFrame] = {}
    for output_name in config_inputs["outputs_to_read"]:
        filename = ixa_inputs["epimodel.GlobalParams"][output_name]["filename"]
        output_path = _resolve_output_path(
            output_dir=output_dir,
            filename=filename,
        )
        outputs[output_name] = pl.read_csv(output_path)
    return outputs


def execute_phase1_model(
    model_inputs: dict[str, Any],
) -> dict[str, pl.DataFrame]:
    """Run the full phase-1 model workflow and return requested reports."""
    config_inputs = model_inputs["config_inputs"]
    output_dir = Path(config_inputs["output_dir"])
    output_dir.mkdir(parents=True, exist_ok=True)

    input_file_path = write_input_json(
        model_inputs["ixa_inputs"],
        output_dir=output_dir,
    )
    generate_importation_timeseries(model_inputs)
    run_ixa_model(
        config_inputs=config_inputs,
        input_file_path=input_file_path,
    )
    return read_model_outputs(model_inputs)


def phase1_report_to_rows(report: pl.DataFrame) -> dict[str, list[Any]]:
    """Convert the canonical phase-1 report to the row mapping expected by MRP."""
    return {column: report[column].to_list() for column in report.columns}


def phase1_rows_to_report(
    report: pl.DataFrame | dict[str, list[Any]],
) -> pl.DataFrame:
    """Normalize a phase-1 report payload into a Polars DataFrame."""
    if isinstance(report, pl.DataFrame):
        return report
    return pl.DataFrame(report)


def write_canonical_output_csv(
    output_path: str | Path,
    outputs: dict[str, pl.DataFrame],
) -> None:
    """Persist the single phase-1 cloud/local task artifact."""
    report = outputs[PHASE1_OUTPUT_NAME]
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    report.write_csv(output_path)
