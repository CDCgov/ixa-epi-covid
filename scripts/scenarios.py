from datetime import date
import json
from pathlib import Path
import subprocess

import polars as pl
import matplotlib.pyplot as plt
import requests


input_dir = Path("input")
output_dir = Path("output")

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
        .alias("t"),
        pl.col("inpatient_beds_used_covid").cast(pl.Float64).alias("inpatient_beds_used_covid"),
    )
    .sort("t")
)
print(hosp_data.head())

death_data = (
    pl.read_csv(
        input_dir / "COVID_Deaths_Data.csv",
        try_parse_dates=True,
        infer_schema=False
    )
    .filter(pl.col("state") == "IN")
    .select(["end_date", "new_deaths"])
    .with_columns(
        (pl.col("end_date").str.strptime(pl.Date, "%m/%d/%Y") - pl.lit(date(2020, 1, 1)))
        .dt.total_days()
        .alias("t"),
        pl.col("new_deaths").cast(pl.Float64).alias("new_deaths")
    )
    .sort("t")
)

print(death_data.head())

input_path = "input/input.json"
with open(input_path, "r", encoding="utf-8") as f:
    input_data = json.load(f)
    input_data["epimodel.GlobalParams"]["seed"] = 5555
    input_data["epimodel.GlobalParams"]["synth_population_file"] = "input/in.csv"
    input_data["epimodel.GlobalParams"]["probability_mild_given_infect"] = 0.7
    input_data["epimodel.GlobalParams"]["infectiousness_rate_fn"]["Constant"]["rate"] = 0.22
    input_data["epimodel.GlobalParams"]["settings_properties"]["Home"]["alpha"] = 0.0
    input_data["epimodel.GlobalParams"]["default_modifiers"]["CommunityMobilityReduction"]["community"] = [0.8, 0.0, 0.0, 0.2]
    input_data["epimodel.GlobalParams"]["interventions"][0]["activation_time"] = 78.0 #school
    input_data["epimodel.GlobalParams"]["interventions"][1]["activation_time"] = 83.0 #community
    input_data["epimodel.GlobalParams"]["interventions"][2]["activation_time"] = 83.0 #workplace
    input_data["epimodel.GlobalParams"]["interventions"][0]["acceptance_probability"] = 1.0 #school
    input_data["epimodel.GlobalParams"]["interventions"][1]["acceptance_probability"] = 1.0 #community
    input_data["epimodel.GlobalParams"]["interventions"][2]["acceptance_probability"] = 0.5 #workplace
    input_data["epimodel.GlobalParams"]["critical_to_dead_delay"]["mu"] = 1.5
    input_data["epimodel.GlobalParams"]["imported_cases_timeseries"]["filename"] = "input/importation_test.csv" 

    with open(input_path, "w", encoding="utf-8") as out_f:
        json.dump(input_data, out_f, indent=2)

result = subprocess.run(
    ["target/release/ixa-epi-covid", "-c", "input/input.json", "-o", "output", "-f"],
    check=True,
)

if result.returncode != 0:
    raise RuntimeError("System command failed")
if result.returncode != 0:
    raise RuntimeError("System command failed")


attack_rate = pl.read_csv(output_dir / "attack_rate_report.csv")
hosp_out = pl.read_csv(output_dir / "current_hospitalizations_report.csv")
death_out = pl.read_csv(output_dir / "aggregated_deaths_report.csv")


data_ar = pl.DataFrame(
    {
        "t_upper": [115, 120, 125],
        "age_group": ["Age0To39", "Age40To59", "Age60Plus"],
        "attack_rate": [0.0305, 0.0314, 0.0165],
        "ar_low": [0.019, 0.019, 0.01],
        "ar_high": [0.043, 0.05, 0.024],
    }
)

col_pal = {
    "Age0To39": "#7fc97f",
    "Age40To59": "#beaed4",
    "Age60Plus": "#fdc086",
}


# -----------------------
# Attack-rate plot
fig, ax = plt.subplots()

for age_group in attack_rate["age_group"].unique().to_list():
    group = (
        attack_rate
        .filter(pl.col("age_group") == age_group)
        .sort("t_upper")
    )

    ax.plot(
        group["t_upper"].to_list(),
        group["attack_rate"].to_list(),
        color=col_pal[age_group],
        linewidth=1.0,
        label=age_group,
    )

for row in data_ar.iter_rows(named=True):
    ax.errorbar(
        row["t_upper"],
        row["attack_rate"],
        yerr=[
            [row["attack_rate"] - row["ar_low"]],
            [row["ar_high"] - row["attack_rate"]],
        ],
        fmt="o",
        color=col_pal[row["age_group"]],
        capsize=4,
    )

ax.legend()
fig.tight_layout()
plt.show()


# -----------------------
# Hospitalization and cases plot
fig, ax = plt.subplots()

ax.plot(
    hosp_data["t"].to_list(),
    hosp_data["inpatient_beds_used_covid"].to_list(),
    color="black",
)

ax.plot(
    hosp_out["t_upper"].to_list(),
    hosp_out["current_hospitalizations"].to_list(),
    color="red",
    label="hosp",
)

ax.set_xlim(0, 200)
ax.legend(title="")
fig.tight_layout()
plt.show()

# -----------------------
# Deaths plot
fig, ax = plt.subplots()

ax.plot(
    death_data["t"].to_list(),
    death_data["new_deaths"].to_list(),
    color="black",
    label="Reported Deaths",
)

ax.plot(
    death_out["t_upper"].to_list(),
    death_out["deaths"].to_list(),
    color="red",
    label="Modelled Deaths",
)

ax.set_xlim(0, 200)
ax.legend(title="")
fig.tight_layout()
plt.show()