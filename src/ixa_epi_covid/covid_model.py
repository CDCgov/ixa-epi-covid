import json
import subprocess
from pathlib import Path
from typing import Any

import polars as pl
from importation import ImportationModel, get_linelist_data
from mrp import MRPModel

PHASE1_OUTPUT_NAME = "aggregated_deaths_report"
CANONICAL_OUTPUT_FILENAME = "output.csv"


class CovidModel(MRPModel):
    """IXA COVID model adapter used by local calibration and MRP tasks."""

    def run(self) -> None:
        """Run the MRP task and write the canonical phase-1 CSV output."""
        outputs = self.simulate(self.input)
        report = outputs[PHASE1_OUTPUT_NAME]
        self.write_csv(
            CANONICAL_OUTPUT_FILENAME,
            {column: report[column].to_list() for column in report.columns},
        )

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> dict[str, pl.DataFrame]:
        """Run one phase-1 model simulation and return configured reports."""
        ixa_inputs = model_inputs["ixa_inputs"]
        config_inputs = model_inputs["config_inputs"]
        importation_inputs = model_inputs["importation_inputs"]

        output_dir = Path(config_inputs["output_dir"])
        output_dir.mkdir(parents=True, exist_ok=True)

        # Write the IXA inputs for downstream error reproduction.
        input_file_path = output_dir / "input.json"
        ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
            ixa_inputs["epimodel.GlobalParams"]["seed"]
        )
        input_file_path.write_text(
            json.dumps(ixa_inputs, indent=4),
            encoding="utf-8",
        )

        global_params = ixa_inputs["epimodel.GlobalParams"]

        # Calculate the probability that an individual will die given that
        # they are symptomatic. This assumes individuals are evenly
        # distributed by age group.
        n_age_groups = len(global_params["symptom_age_groups"])
        case_fatality_ratio = (
            sum(global_params["probability_severe_given_mild"].values())
            / n_age_groups
            * sum(global_params["probability_critical_given_severe"].values())
            / n_age_groups
            * sum(global_params["probability_dead_given_critical"].values())
            / n_age_groups
        )

        proportion_symptomatic = global_params["probability_mild_given_infect"]

        importation_params = {
            "symptomatic_reporting_prob": importation_inputs[
                "symptomatic_reporting_prob"
            ],
            "case_fatality_ratio": case_fatality_ratio,
            "proportion_asymptomatic": 1.0 - proportion_symptomatic,
        }

        importation_filename = Path(
            global_params["imported_cases_timeseries"]["filename"]
        )

        importation_model = ImportationModel(
            data=get_linelist_data(),
            parameters=importation_params,
            national_model="multinomial",
            state_model="proportional",
            seed=global_params["seed"],
        )

        timeseries_data = importation_model.sample_state_importation_incidence(
            state=importation_inputs["state"],
            year=importation_inputs.get("year"),
        )
        timeseries_data.write_csv(importation_filename)

        cmd = [
            config_inputs["exe_file"],
            "--config",
            str(input_file_path),
            "--output",
            str(output_dir),
            "--force-overwrite",
            "--no-stats",
        ]

        try:
            subprocess.run(cmd, capture_output=True, check=True)
        except subprocess.CalledProcessError as e:
            print("Error running the ixa model:")
            print("Command:", " ".join(cmd))
            print("Return code:", e.returncode)
            print("Standard error:", e.stderr)
            raise e

        outputs = {}
        for output_name in config_inputs["outputs_to_read"]:
            filename = global_params[output_name]["filename"]
            output_path = output_dir / filename
            if output_path.exists():
                outputs[output_name] = pl.read_csv(output_path)
            elif Path(filename).exists():
                outputs[output_name] = pl.read_csv(Path(filename))
            else:
                raise FileNotFoundError(
                    f"Expected output file {filename} not found. "
                    f"Looked in {output_dir}."
                )
        return outputs


def main() -> int:
    """Run the model from ``python -m ixa_epi_covid.covid_model``."""
    CovidModel().run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
