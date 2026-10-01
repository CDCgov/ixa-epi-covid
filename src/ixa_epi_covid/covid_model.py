import json
import subprocess
from pathlib import Path
from typing import Any

import polars as pl
from importation import ImportationModel, get_linelist_data
from mrp import MRPModel
from scipy.stats import poisson

from pathlib import Path
# Or Path("/app") for a specific directory

FOLDERS_TO_PRINT = [
    Path("/app/experiments"),
    Path("/app/scripts"),
]

IGNORED_DIRECTORIES = {".venv"}

def print_tree(directory, prefix=""):
    try:
        entries = sorted(
            (
                entry for entry in directory.iterdir()
                if not (
                    entry.is_dir()
                    and entry.name in IGNORED_DIRECTORIES
                )
            ),
            key=lambda p: (p.is_file(), p.name.lower())
        )
    except PermissionError:
        print(prefix + "[permission denied]")
        return

    for index, entry in enumerate(entries):
        is_last = index == len(entries) - 1
        connector = "└── " if is_last else "├── "
        print(prefix + connector + entry.name)

        if entry.is_dir():
            extension = "    " if is_last else "│   "
            print_tree(entry, prefix + extension)



class CovidModel(MRPModel):
    def run(self):
        pass

    @staticmethod
    def simulate(model_inputs: dict[str, Any]) -> pl.DataFrame:
        ixa_inputs = model_inputs["ixa_inputs"]
        config_inputs = model_inputs["config_inputs"]
        importation_inputs = model_inputs["importation_inputs"]
        config_inputs["output_dir"] = f"./{config_inputs['output_dir']}"
        # Write the ixa inputs to the specified file location so that downstream errors can be re-tried
        input_file_path = Path("./", config_inputs["output_dir"], "input.json")
        print(input_file_path)
        print(str(input_file_path))
        ixa_inputs["epimodel.GlobalParams"]["seed"] = int(
            ixa_inputs["epimodel.GlobalParams"]["seed"]
        )
        ixa_inputs["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"] = f"./{ixa_inputs['epimodel.GlobalParams']['imported_cases_timeseries']['filename']}"
        ixa_inputs["epimodel.GlobalParams"]["synth_population_file"] = f"./{ixa_inputs['epimodel.GlobalParams']['synth_population_file']}"
        with open(input_file_path, "w") as f:
            json.dump(ixa_inputs, f, indent=4)
        print("ixa inputs")
        print(ixa_inputs)
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
            "./" + str(input_file_path),
            "--output",
            config_inputs["output_dir"],
            "--force-overwrite",
            "--no-stats",
        ]
        import os
        try:
            print("Working directory:", os.getcwd())
            print("Files:", os.listdir("."))
            print(f"CONFIG OUTPUTS {config_inputs}")
            for folder in FOLDERS_TO_PRINT:
                print(f"\n{folder}")
                print_tree(folder)
            
            print("files in directory")
            subprocess.run(["pwd"])
            subprocess.run(["ls", "-l"])
            print(f"files in {str(config_inputs["output_dir"])}")
            subprocess.run(["ls", "-l", str(config_inputs["output_dir"])])
            subprocess.run(cmd, capture_output=True, check=True)
        except subprocess.CalledProcessError as e:
            print("Error running the ixa model:")
            print("Command:", " ".join(cmd))
            print("Return code:", e.returncode)
            print("Standard output:", e.stdout)
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
    
    def outputs_to_distance(
        model_output: dict[str, pl.DataFrame], target_data: pl.DataFrame
    ) -> float:
        """
        Calculates the weighted error between current hospitalizations, weekly incident deaths, and cumulative age-specific attack rates.

        Args:
            model_output (dict[str, pl.DataFrame]): A dictionary containing the model outputs as Polars DataFrames.
            target_data (pl.DataFrame): The observed current hospitalizations, weekly incident deaths and age-specific attack rates.
        Returns:
            float: The calculated distance.
        """
        def poisson_lhood(model, data):
            return -poisson.logpmf(data, model + 1e-6)
        deaths = (
            model_output["aggregated_deaths_report"]
        )
        print(model_output)
        hosp = (
            model_output["current_hospitalizations_report"]
        )
        ar_039 = (
            model_output["attack_rate_report"]
            .filter(pl.col("age_group") == "Age0To39")
            .filter(pl.col("t_upper") == 120)
        )
        ar_4059 = (
            model_output["attack_rate_report"]
            .filter(pl.col("age_group") == "Age40To59")
            .filter(pl.col("t_upper") == 120)
        )
        ar_60plus = (
            model_output["attack_rate_report"]
            .filter(pl.col("age_group") == "Age60Plus")
            .filter(pl.col("t_upper") == 120)
        )
        target_deaths = target_data.filter(pl.col("data_type") == "deaths")
        target_hosp = target_data.filter(pl.col("data_type") == "hospitalizations")
        target_ar_039 = target_data.filter(pl.col("data_type") == "Age0To39AR")
        target_ar_4059 = target_data.filter(pl.col("data_type") == "Age40To59AR")
        target_ar_60plus = target_data.filter(pl.col("data_type") == "Age60PlusAR")

        joint_deaths = (
            deaths.select(pl.col(["t_upper", "deaths"]))
            .join(
                target_deaths.select(pl.col(["t_upper", "count"])),
                on="t_upper",
                how="right",
            )
            .with_columns(pl.col("deaths").fill_null(strategy="zero"))
        .with_columns(
            pl.struct(["deaths", "count"])
            .map_elements(
                lambda x: poisson_lhood(x["deaths"], x["count"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
        )

        joint_hosp = (
            hosp.select(pl.col(["t_upper", "current_hospitalizations"]))
            .join(
                target_hosp.select(pl.col(["t_upper", "count"])),
                on="t_upper",
                how="right",
            )
            .with_columns(pl.col("current_hospitalizations").fill_null(strategy="zero"))
        .with_columns(
            pl.struct(["current_hospitalizations", "count"])
            .map_elements(
                lambda x: poisson_lhood(x["current_hospitalizations"], x["count"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
        )
        def ar_difference(model_ar: pl.DataFrame, target_ar: pl.DataFrame) -> float:
            model_value = (
            model_ar.select(pl.col("attack_rate")).item()
            if model_ar.height > 0
            else 0.0
            )
            target_lower = target_ar.select(pl.col("lower")).item()
            target_upper = target_ar.select(pl.col("upper")).item()
            sigma = (target_upper - target_lower) / 3.92
            target_value = target_ar.select(pl.col("count")).item()
            return float(((model_value - target_value) ** 2)/(2 * sigma ** 2))

        ar_diff_039 = ar_difference(ar_039, target_ar_039)
        ar_diff_4059 = ar_difference(ar_4059, target_ar_4059)
        ar_diff_60plus = ar_difference(ar_60plus, target_ar_60plus)
        ar_distance = ar_diff_039 + ar_diff_4059 + ar_diff_60plus

        deaths_distance = joint_deaths["negloglikelihood"].mean()
        hosp_distance = joint_hosp["negloglikelihood"].mean()
        print(f"Deaths distance: {deaths_distance}, Hospitalizations distance: {hosp_distance}, Attack rate distance: {ar_distance}")
        return float(deaths_distance + hosp_distance + ar_distance)

