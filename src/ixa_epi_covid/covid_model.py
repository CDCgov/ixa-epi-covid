import json
import subprocess
from pathlib import Path
from typing import Any

import polars as pl
from importation import ImportationModel, get_linelist_data
from mrp import MRPModel


class CovidModel(MRPModel):
    def run(self):
        pass

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> pl.DataFrame:
        ixa_inputs = model_inputs["ixa_inputs"]
        config_inputs = model_inputs["config_inputs"]
        importation_inputs = model_inputs["importation_inputs"]

        # Write the ixa inputs to the specified file location so that downstream errors can be re-tried
        input_file_path = Path(config_inputs["output_dir"], "input.json")
        ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
            ixa_inputs["epimodel.GlobalParams"]["seed"]
        )
        with open(input_file_path, "w") as f:
            json.dump(ixa_inputs, f, indent=4)

        ## Generate the importation time series from relevant ixa parameters --------------
        # Calculate the probability that an inidivdual will die given that they are symptomatic
        # This calculation assumes that individuals are evenly distributed by age group
        n_age_groups = len(
            ixa_inputs["epimodel.GlobalParams"]["symptom_age_groups"]
        )
        case_fatality_ratio = (
            sum(
                ixa_inputs["epimodel.GlobalParams"][
                    "probability_severe_given_mild"
                ].values()
            )
            / n_age_groups
            * sum(
                ixa_inputs["epimodel.GlobalParams"][
                    "probability_critical_given_severe"
                ].values()
            )
            / n_age_groups
            * sum(
                ixa_inputs["epimodel.GlobalParams"][
                    "probability_dead_given_critical"
                ].values()
            )
            / n_age_groups
        )

        proportion_symptomatic = ixa_inputs["epimodel.GlobalParams"][
            "probability_mild_given_infect"
        ]
        symptomatic_reporting_prob = model_inputs["importation_inputs"][
            "symptomatic_reporting_prob"
        ]

        importation_params = {
            "symptomatic_reporting_prob": symptomatic_reporting_prob,
            "case_fatality_ratio": case_fatality_ratio,
            "proportion_asymptomatic": 1.0 - proportion_symptomatic,
        }

        importation_filename = ixa_inputs["epimodel.GlobalParams"][
            "imported_cases_timeseries"
        ]["filename"]

        # Create the model object
        importation_model = ImportationModel(
            data=get_linelist_data(),
            parameters=importation_params,
            national_model="multinomial",
            state_model="proportional",
            seed=ixa_inputs["epimodel.GlobalParams"][
                "seed"
            ],  # Optional argument to set the model seed
        )

        # Generate timeseries data from the model object for state and optional year
        timeseries_data = importation_model.sample_state_importation_incidence(
            state=importation_inputs["state"],
            year=importation_inputs.get("year"),
        )

        # Store timeseries at appropriate location accessible to ixa
        timeseries_data.write_csv(importation_filename)

        ## Run the ixa transmission model ------------------------
        # Write command to call the ixa model binaries
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
            subprocess.run(cmd, capture_output=True, check=True)
        except subprocess.CalledProcessError as e:
            print("Error running the ixa model:")
            print("Command:", " ".join(cmd))
            print("Return code:", e.returncode)
            print("Standard error:", e.stderr)
            raise e

        # Read the model incidence report from the specified location and return as a DataFrame
        outputs = {}
        for output in config_inputs["outputs_to_read"]:
            fp = ixa_inputs["epimodel.GlobalParams"][output]["filename"]
            if Path(config_inputs["output_dir"], fp).exists():
                outputs.update(
                    {
                        output: pl.read_csv(
                            Path(config_inputs["output_dir"], fp)
                        )
                    }
                )
            elif Path(fp).exists():
                outputs.update({output: pl.read_csv(Path(fp))})
            else:
                raise FileNotFoundError(
                    f"Expected output file {fp} not found. Looked in {config_inputs['output_dir']}"
                )
        return outputs
