from .covid_model import CovidModel
import argparse
from importation import ImportationModel, get_linelist_data
from pathlib import Path
import subprocess
import json

def run(ixa_config: Path, output_dir: Path):
    with open(ixa_config, "r") as f:
        ixa_inputs = json.load(f)

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

    print(importation_params)

    importation_filename = ixa_inputs["epimodel.GlobalParams"][
        "imported_cases_timeseries"
    ]["filename"]
    model = CovidModel()

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
        state="IN",
        year=2020,
    )

    # Store timeseries at appropriate location accessible to ixa
    timeseries_data.write_csv(importation_filename)

    ## Run the ixa transmission model ------------------------
    # Write command to call the ixa model binaries
    cmd = [
        "./target/release/ixa-epi-covid",
        "--config",
        ixa_config,
        "--output",
        output_dir,
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


argparser = argparse.ArgumentParser(description="Run the ixa-epi-covid model with the specified configuration file and output directory.")

argparser.add_argument(
    "-c",
    "--ixa_config",
    type=Path,
    required=True,
    help="Path to the ixa configuration file (input.json) containing the model parameters.",
)

argparser.add_argument(
    "-o",
    "--output_dir",
    type=Path,
    required=True,
    help="Path to the output directory where model results will be saved.",
)

if __name__ == "__main__":
    args = argparser.parse_args()
    run(args.ixa_config, args.output_dir)