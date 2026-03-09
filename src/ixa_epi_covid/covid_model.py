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
        input_file_path = Path(config_inputs["output_dir"],"input.json")
        ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
            ixa_inputs["epimodel.GlobalParams"]["seed"]
        )
        with open(input_file_path, "w") as f:
            json.dump(ixa_inputs, f, indent=4)

        ## Generate the importation time series from relevant ixa parameters --------------
        # Calculate the probability that an inidivdual will die given that they are symptomatic
        case_fatality_ratio = (
            ixa_inputs["epimodel.GlobalParams"][
                "probability_severe_given_mild"
            ]
            * ixa_inputs["epimodel.GlobalParams"][
                "probability_critical_given_severe"
            ]
            * ixa_inputs["epimodel.GlobalParams"][
                "probability_dead_given_critical"
            ]
        )

        proportion_asymptomatic = ixa_inputs["epimodel.GlobalParams"][
            "probability_mild_given_infect"
        ]
        symptomatic_reporting_prob = ixa_inputs["epimodel.GlobalParams"][
            "symptomatic_reporting_prob"
        ]

        importation_params = {
            "symptomatic_reporting_prob": symptomatic_reporting_prob,
            "case_fatality_ratio": case_fatality_ratio,
            "proportion_asymptomatic": proportion_asymptomatic,
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
            "-f",
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
        incidence_report_filename = ixa_inputs["epimodel.GlobalParams"][
            "incidence_report"
        ]["filename"]
        incidence_report_path = Path(
            config_inputs["output_dir"], incidence_report_filename
        )

        return pl.read_csv(incidence_report_path)
