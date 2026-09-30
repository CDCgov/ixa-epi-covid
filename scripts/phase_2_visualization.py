from datetime import date
import pickle
from calibrationtools.calibration_results import CalibrationResults, Particle
from pathlib import Path
import os
import requests
import seaborn as sns
import matplotlib.pyplot as plt
import numpy as np
import polars as pl
import json
import tempfile
import polars as pl
import os
from ixa_epi_covid import CovidModel
from mrp.api import apply_dict_overrides
import multiprocessing as mp
from concurrent.futures import ThreadPoolExecutor
from typing import Any
import shutil
from scipy.stats  import poisson

def outputs_to_distance(deaths, hosp, ar, target_data: pl.DataFrame
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
        ar_039 = (
            ar
            .filter(pl.col("age_group") == "Age0To39")
            .filter(pl.col("t_upper") == 120)
        )
        ar_4059 = (
           ar
            .filter(pl.col("age_group") == "Age40To59")
            .filter(pl.col("t_upper") == 120)
        )
        ar_60plus = (
            ar
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
        return float(deaths_distance + hosp_distance + ar_distance)


wd = Path("experiments", "phase2")
results_name = "test"

calibration_dir = wd / "calibration" / results_name
projection_dir = wd / "projection" / results_name

# Load results object from the calibration directory
with open(calibration_dir / "results.pkl", "rb") as fp:
    results: CalibrationResults = pickle.load(fp)

diagnostics = results.get_diagnostics()
print(
    json.dumps(
        {
            k1: {k2: np.format_float_positional(v2, precision=3) for k2, v2 in v1.items()}
            for k1, v1 in diagnostics["quantiles"].items()
        },
        indent=4,
    )
)

posterior_samples = results.sample_posterior_particles(n=int(results.ess))
distance = results.distance_history



# for param in results.fitted_params:
#     vals = [p[param] for p in posterior_samples]
#     min_val = min(vals)
#     max_val = max(vals)

#     sns.histplot(x=vals, stat="density", kde=True, color='orange', edgecolor='black')
#     eval_points = np.arange(
#         min_val - np.var(vals), max_val + np.var(vals), 0.01
#     )
#     param_prior = None
#     for prior in results.priors.priors:
#         if prior.param == param:
#             param_prior = prior
#             break
#     if not param_prior:
#         raise (ValueError, f"Could not find prior {param}")

#     density_vals = [
#         param_prior.probability_density(Particle({param: v}))
#         for v in eval_points
#     ]

#     sns.lineplot(
#         data=pl.DataFrame({param: list(eval_points), "density": density_vals}),
#         x=param,
#         y="density",
#     )
#     plt.title(f"Posterior versus prior distribution")
#     plt.xlabel(" ".join(param.split(".")))
#     plt.ylabel("Density")
#     plt.tight_layout()
#     plt.show()

HOSPITALIZATION_API_URL = "https://healthdata.gov/api/v3/views/g62h-syeh/query.json"

def apply_dtypes(df, dtypes):
    """
    Cast dataframe columns using a dtype dictionary.
    """
    casts = [
        pl.col(column).cast(dtype, strict=False)
        for column, dtype in dtypes.items()
        if column in df.columns
    ]
    return df.with_columns(casts) if casts else df

def load_hospitalization_data():
    """
    Load hospitalization data from the CDC API.
    """
    params = {
            "query": "SELECT * WHERE `state` = 'IN'"
        }
    response = requests.get(HOSPITALIZATION_API_URL, params=params, timeout=60)
    response.raise_for_status()
    df = pl.DataFrame(response.json())
    HOSPITALIZATION_DTYPES = {
    "state": pl.String,
    "date": pl.String,
        **{
            column: pl.Float64
            for column in df.columns
            if column not in {"state", "date"}
        },
    }
    
    df = apply_dtypes(df, HOSPITALIZATION_DTYPES)
    df = df.with_columns(
            pl.col("date").str.to_date("%Y-%m-%dT%H:%M:%S%.3f")
        )
    return df

# -----------------------
# Hospitalization data
hosp_data = (
    load_hospitalization_data()
    .filter(pl.col("state") == "IN")
    .select(["date", "inpatient_beds_used_covid"])
    .with_columns(
        (pl.col("date").cast(pl.Date) - pl.lit(date(2020, 1, 1)))
        .dt.total_days()
        .cast(pl.Float64)
        .alias("t_upper"),
        pl.col("inpatient_beds_used_covid").cast(pl.Float64).alias("inpatient_beds_used_covid"),
    )
    .sort("t_upper")
    .filter(pl.col("t_upper")<120)
)
target_hosp_data = (
    hosp_data.drop("date")
    .rename({"inpatient_beds_used_covid": "count"})
    .with_columns(pl.lit("hospitalizations").alias("data_type"))
)

death_data = (
    pl.read_csv(
        "input/COVID_Deaths_Data.csv",
        try_parse_dates=True,
        infer_schema=False
    )
    .filter(pl.col("state") == "IN")
    .select(["end_date", "new_deaths"])
    .with_columns(
        (pl.col("end_date").str.strptime(pl.Date, "%m/%d/%Y") - pl.lit(date(2020, 1, 1)))
        .dt.total_days()
        .cast(pl.Float64)
        .alias("t_upper"),
        pl.col("new_deaths").cast(pl.Float64).alias("new_deaths")
    )
    .sort("t_upper")
    .filter(pl.col("t_upper")<120)
)
target_death_data = (
    death_data.drop("end_date")
    .rename({"new_deaths": "count"})
    .with_columns(pl.lit("deaths").alias("data_type"))
)

target_attack_rate_data = pl.DataFrame(
    {
        "t_upper": [120.0, 120.0, 120.0],
        "count": [0.0305, 0.0314, 0.0165],
        "lower": [0.019, 0.019, 0.01],
        "upper": [0.043, 0.05, 0.024],
        "data_type": ["Age0To39AR", "Age40To59AR", "Age60PlusAR"],
    },
    schema={"t_upper": pl.Float64, "count": pl.Float64, "lower": pl.Float64, "upper": pl.Float64, "data_type": pl.String},
)

target_data = pl.concat(
    [
        df.select(
            pl.col("t_upper").cast(pl.Float64),
            pl.col("count").cast(pl.Float64),
            pl.col("data_type").cast(pl.String),
            pl.col("lower").cast(pl.Float64) if "lower" in df.columns else pl.lit(None).cast(pl.Float64).alias("lower"),
            pl.col("upper").cast(pl.Float64) if "upper" in df.columns else pl.lit(None).cast(pl.Float64).alias("upper"),
        )
        for df in (target_hosp_data, target_death_data, target_attack_rate_data)
    ],
    how="vertical",
).sort(["data_type", "t_upper"])

calibration_path = calibration_dir / "simulations"
aggregated_deaths_report_list = []
imported_cases_timeseries_list = []
current_hospitalizations_report_list = []
attack_rate_report_list = []
for particle_dir in calibration_path.iterdir():
    if particle_dir.is_dir():
        
        imported_cases_timeseries = pl.read_csv(particle_dir / "imported_cases_timeseries.csv")
        aggregated_deaths_report = pl.read_csv(particle_dir / "aggregated_deaths_report.csv")
        current_hospitalizations_report = pl.read_csv(particle_dir / "current_hospitalizations_report.csv")
        attack_rate_report = pl.read_csv(particle_dir / "attack_rate_report.csv")
        imported_cases_timeseries = imported_cases_timeseries.with_columns(
            pl.col("imported_infections").cum_sum().over(order_by="time").alias("cumulative_imported_infections"),
            pl.lit(particle_dir.name).alias("seed")
        )
        aggregated_deaths_report = aggregated_deaths_report.with_columns(
            pl.lit(particle_dir.name).alias("seed")
        )
        current_hospitalizations_report = current_hospitalizations_report.with_columns(
            pl.lit(particle_dir.name).alias("seed")
        )
        attack_rate_report = attack_rate_report.with_columns(
            pl.lit(particle_dir.name).alias("seed")
        ).filter(pl.col("t_upper") == 120.0)

        distance = outputs_to_distance(aggregated_deaths_report, current_hospitalizations_report, attack_rate_report, target_data)
        if distance < 250:
            aggregated_deaths_report = aggregated_deaths_report.with_columns(
                pl.lit(distance).alias("distance")
            )
            current_hospitalizations_report = current_hospitalizations_report.with_columns(
                pl.lit(distance).alias("distance")
            )
            attack_rate_report = attack_rate_report.with_columns(
                pl.lit(distance).alias("distance")
            )

            
            imported_cases_timeseries_list.append(imported_cases_timeseries)
            aggregated_deaths_report_list.append(aggregated_deaths_report)
            current_hospitalizations_report_list.append(current_hospitalizations_report)
            attack_rate_report_list.append(attack_rate_report)


death_data = pl.concat(aggregated_deaths_report_list)
imported_data = pl.concat(imported_cases_timeseries_list)
current_hospitalizations_data = pl.concat(current_hospitalizations_report_list)
attack_rate_data = pl.concat(attack_rate_report_list)

sns.scatterplot(
    data=imported_data.filter(pl.col('imported_infections') > 0),
    x="time",
    y="cumulative_imported_infections",
    alpha=0.2,
)
sns.lineplot(
    data=imported_data,
    x="time",
    y="cumulative_imported_infections",
    estimator='median',
)
plt.xlabel("Time")
plt.ylabel("Cumulative Imported Infections")
plt.show()

hosp_x_col = "t_upper"
hosp_y_col = "current_hospitalizations"

fig, ax = plt.subplots()
sns.lineplot(
    data=current_hospitalizations_data.sort(["seed", hosp_x_col]),
    x=hosp_x_col,
    y=hosp_y_col,
    units="seed",
    estimator=None,
    color="blue",
    alpha=0.3,
    linewidth=0.8,
    ax=ax,
)
sns.lineplot(
    data=target_hosp_data,
    x="t_upper",
    y="count",
    color="red",
    linewidth=2,
    label="Observed hospitalizations",
    ax=ax,
)
ax.set_xlabel("Time")
ax.set_ylabel("Current Hospitalizations")
ax.set_title("Simulated versus observed hospitalizations")
plt.tight_layout()
plt.show()

death_x_col = "t_upper"
death_y_col = "deaths"

fig, ax = plt.subplots()
sns.lineplot(
    data=death_data.sort(["seed", death_x_col]),
    x=death_x_col,
    y=death_y_col,
    units="seed",
    estimator=None,
    color="blue",
    alpha=0.3,
    linewidth=0.8,
    ax=ax,
)
sns.lineplot(
    data=target_death_data,
    x="t_upper",
    y="count",
    color="red",
    linewidth=2,
    label="Observed deaths",
    ax=ax,
)
ax.set_xlabel("Time")
ax.set_ylabel("Deaths")
ax.set_title("Simulated versus observed deaths")
plt.tight_layout()
plt.show()

age_groups = ["Age0To39", "Age40To59", "Age60Plus"]

fig, axes = plt.subplots(1, len(age_groups), figsize=(5 * len(age_groups), 4))
for ax, age_group in zip(np.atleast_1d(axes), age_groups):
    ar_vals = attack_rate_data.filter(pl.col("age_group") == age_group)["attack_rate"].to_numpy()
    sns.histplot(x=ar_vals, stat="density", color="orange", edgecolor="black", ax=ax)

    target_value = target_attack_rate_data.filter(
        pl.col("data_type") == f"{age_group}AR"
    )["count"].item()
    ax.axvline(target_value, color="red", linestyle="--", linewidth=2, label="Observed")

    ax.set_title(age_group)
    ax.set_xlabel("Attack Rate")
    ax.set_ylabel("Density")
    ax.legend()

fig.suptitle("Simulated attack rates versus observed")
plt.tight_layout()
plt.show()