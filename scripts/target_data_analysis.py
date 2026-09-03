import requests
from pathlib import Path
import matplotlib.pyplot as plt
import polars as pl
import seaborn as sns

CASE_STATE_API_URL = "https://data.cdc.gov/api/v3/views/pwn4-m3yp/query.json"
CASE_COUNTY_API_URL = "https://data.cdc.gov/api/v3/views/yviw-z6j5/query.json"
PROVISIONAL_DEATHS_API_URL = "https://data.cdc.gov/api/v3/views/r8kw-7aab/query.json"
HOSPITALIZATION_API_URL = "https://healthdata.gov/api/v3/views/g62h-syeh/query.json"
LINE_LIST_API_URL = "https://data.cdc.gov/api/v3/views/n8mc-b4w4/query.json"

CASE_STATE_DTYPES = {
    "date_updated": pl.String,
    "state": pl.String,
    "start_date": pl.String,
    "end_date": pl.String,
    "tot_cases": pl.Float64,
    "new_cases": pl.Float64,
    "tot_deaths": pl.Float64,
    "new_deaths": pl.Float64,
    "new_historic_cases": pl.Float64,
    "new_historic_deaths": pl.Float64,
}

CASE_COUNTY_DTYPES = {
    "fips_code": pl.String,
    "county": pl.String,
    "state": pl.String,
    "state_fips": pl.String,
    "date": pl.String,
    "cumulative_cases": pl.Float64,
    "cumulative_deaths": pl.Float64,
    "new_cases": pl.Float64,
    "new_deaths": pl.Float64,
}

PROVISIONAL_DEATHS_DTYPES = {
    "data_as_of": pl.String,
    "start_date": pl.String,
    "end_date": pl.String,
    "group": pl.String,
    "year": pl.Int64,
    "mmwr_week": pl.String,
    "month": pl.String,
    "week_ending_date": pl.String,
    "state": pl.String,
    "covid_19_deaths": pl.Float64,
    "total_deaths": pl.Float64,
    "percent_of_expected_deaths": pl.Float64,
    "pneumonia_deaths": pl.Float64,
    "pneumonia_and_covid_19_deaths": pl.Float64,
    "influenza_deaths": pl.Float64,
    "pneumonia_influenza_or_covid_19_deaths": pl.Float64,
}

LINE_LIST_DTYPES = {
    "case_month": pl.String,
    "res_state": pl.String,
    "state_fips_code": pl.String,
    "res_county": pl.String,
    "county_fips_code": pl.String,
    "age_group": pl.String,
    "sex": pl.String,
    "race": pl.String,
    "ethnicity": pl.String,
    "case_positive_specimen": pl.String,
    "process": pl.String,
    "exposure_yn": pl.String,
    "current_status": pl.String,
    "symptom_status": pl.String,
    "hosp_yn": pl.String,
    "icu_yn": pl.String,
    "death_yn": pl.String,
}

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

def load_case_state_data():
    """
    Load case state data from the CDC API.
    """
    response = requests.get(CASE_STATE_API_URL, timeout=60)
    response.raise_for_status()
    df = apply_dtypes(pl.DataFrame(response.json()).drop(
        [":id", ":version", ":created_at", ":updated_at"],
        strict=False,
    ), CASE_STATE_DTYPES)
    df = df.with_columns(
        pl.col(["date_updated", "start_date", "end_date"]).str.to_date("%Y-%m-%dT%H:%M:%S%.3f")
    )
    return df

def load_case_county_data():
    """
    Load case county data from the CDC API.
    """
    params = {
                "query": "SELECT * WHERE `state` = 'IN'"
            }
    response = requests.get(CASE_COUNTY_API_URL, params=params, timeout=60)
    response.raise_for_status()
    df =  apply_dtypes(pl.DataFrame(response.json()), CASE_COUNTY_DTYPES)
    df = df.with_columns(
        pl.col("date").str.to_date("%Y-%m-%dT%H:%M:%S%.3f")
    )
    return df

def load_provisional_deaths_data():
    """
    Load provisional deaths data from the CDC API.
    """
    response = requests.get(PROVISIONAL_DEATHS_API_URL, timeout=60)
    response.raise_for_status()
    df = apply_dtypes(pl.DataFrame(response.json()), PROVISIONAL_DEATHS_DTYPES)
    df = df.with_columns(
        pl.col("end_date").str.to_date("%Y-%m-%dT%H:%M:%S%.3f")
    )
    return df

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


def load_line_list_data():
    """
    Load line list data from the CDC API.
    """
    params = {
        "query": "SELECT * WHERE `res_state` = 'IN'"
    }

    response = requests.get(LINE_LIST_API_URL, params=params, timeout=60)
    response.raise_for_status()

    df = apply_dtypes(pl.DataFrame(response.json()), LINE_LIST_DTYPES)
    df = df.with_columns(
        pl.col("case_month").str.to_date("%Y-%MT%H:%M:%S%.3f")
    )
    return df


def visualize_state_cases_and_deaths():
    cases = (
        load_case_state_data()
        .filter(
            (pl.col("end_date") < pl.date(2020, 5, 1))
            & (pl.col("end_date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "IN")
        )
        .sort("end_date")
    )

    hospitalizations = (
        load_hospitalization_data()
        .filter(
            (pl.col("date") < pl.date(2020, 5, 1))
            & (pl.col("date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "IN")
        )
        .with_columns(pl.col("date").dt.truncate("1w").alias("week"))
        .group_by("week")
        .agg(pl.col("inpatient_beds_used_covid").mean())
        .rename({"week": "date"})
        .sort("date")
    )
    
    
    plt.figure(figsize=(10, 6))
    
    sns.lineplot(
        data=cases.to_pandas(), x="end_date", y="new_deaths", label="Weekly Incident Deaths"
    )
    sns.lineplot(
        data=hospitalizations.to_pandas(), x="date", y="inpatient_beds_used_covid", label="Weekly Mean Current Hospitalizations"
    )
    plt.xlabel("Day")
    plt.ylabel("Number of people")
    plt.title("Indiana Incident Deaths and Current Hospitalizations")
    plt.legend()
    plt.tight_layout()
    
    plt.show()

def visualize_state_and_county_cases():
    state = (
        load_case_state_data()
        .filter(
            (pl.col("end_date") < pl.date(2020, 5, 1))
            & (pl.col("end_date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "IN")
        )
        .sort("end_date")
    )

    county = (
        load_case_county_data()
        .filter(
            (pl.col("date") < pl.date(2020, 5, 1))
            & (pl.col("date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "IN")
        )
        .sort("date")
    )
    selected_counties = county.select("county").unique().sample(n=10, seed=42)
    county = county.filter(pl.col("county").is_in(selected_counties["county"]))
    

    plt.figure(figsize=(10, 6))
    sns.lineplot(
        data=state.to_pandas(), x="end_date", y="new_cases", label="Weekly Incident Cases State"
    )
    sns.lineplot(
        data=county.to_pandas(), x="date", y="new_cases", hue = "county"
    )
    
    plt.xlabel("Day")
    plt.ylabel("Number of people")
    plt.title("Indiana Incident Cases State vs Counties")
    plt.legend()
    plt.tight_layout()
    
    plt.show()


def visualize_aggregate_vs_provisional_deaths():
    aggregate = (
        load_case_state_data()
        .filter(
            (pl.col("end_date") < pl.date(2020, 5, 10))
            & (pl.col("end_date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "IN")
        )
        .sort("end_date")
    )

    provisional = (
        load_provisional_deaths_data()
        .filter(
            (pl.col("end_date") < pl.date(2020, 5, 10))
            & (pl.col("end_date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "Indiana")
            & (pl.col("mmwr_week").is_not_null())
        )
        .sort("end_date")
    )
    print(provisional)

    plt.figure(figsize=(10, 6))
    sns.lineplot(
        data=aggregate.to_pandas(), x="end_date", y="new_deaths", label="Weekly Incident Deaths Aggregate"
    )
    sns.lineplot(
        data=provisional.to_pandas(), x="end_date", y="covid_19_deaths", label="Weekly Incident Deaths Provisional"
    )
    
    plt.xlabel("Day")
    plt.ylabel("Number of people")
    plt.title("Indiana Incident Deaths Aggregate vs Provisional")
    plt.legend()
    plt.tight_layout()
    
    plt.show()


def visualize_detailed_hospitalizations():
    hospitalizations = (
        load_hospitalization_data()
        .filter(
            (pl.col("date") < pl.date(2020, 12, 1))
            & (pl.col("date") > pl.date(2020, 3, 1))
            & (pl.col("state") == "IN")
        )
        .sort("date")
    )

    plt.figure(figsize=(10, 6))
    sns.lineplot(
        data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_18_19", label="18–19"
    )
    sns.lineplot(
        data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_20_29", label="20–29"
    )
    sns.lineplot(
            data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_30_39", label="30–39"
        )
    sns.lineplot(
            data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_40_49", label="40–49"
        )
    sns.lineplot(
            data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_50_59", label="50–59"
    )
    sns.lineplot(
            data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_60_69", label="60–69"
        )
    sns.lineplot(
                data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_70_79", label="70–79"
            )
    sns.lineplot(
                data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_adult_covid_confirmed_80", label="80+"
            )
    sns.lineplot(
                data=hospitalizations.to_pandas(), x="date", y="previous_day_admission_pediatric_covid_confirmed", label="Pediatric"
            )
    
    
    plt.xlabel("Day")
    plt.ylabel("Number of people")
    plt.title("Indiana Incident Hospitalizations by Age Group")
    plt.legend()
    plt.tight_layout()
    
    plt.show()

def visualize_line_list_cases():
    line_list = (
        pl.read_csv("line_list_cases_IN.csv", schema_overrides=LINE_LIST_DTYPES)
    )
    line_list = line_list.with_columns(
        pl.col("case_month").str.strptime(
            pl.Date,
            format="%Y-%m",
            strict=False,
        )
    ).filter(
            (pl.col("case_month") <= pl.date(2020, 6, 1))
            & (pl.col("case_month") >= pl.date(2020, 3, 1))
            & (pl.col("res_state") == "IN")
        ).sort("case_month")
   

    age_aggregated = line_list.group_by(["case_month", "age_group"]).agg(
        pl.len()
    )

    race_aggregated = line_list.group_by(["case_month", "race"]).agg(
        pl.len()
    )

    plt.figure(figsize=(10, 6))
    sns.lineplot(
        data=age_aggregated.to_pandas(), x="case_month", y="len", hue="age_group"
    )
    
    plt.xlabel("Day")
    plt.ylabel("Number of people")
    plt.title("Indiana Line List Cases by Age Group")
    plt.legend()
    plt.tight_layout()
    
    plt.show()

    plt.figure(figsize=(10, 6))
    sns.lineplot(
        data=race_aggregated.to_pandas(), x="case_month", y="len", hue="race"
    )
    
    plt.xlabel("Day")
    plt.ylabel("Number of people")
    plt.title("Indiana Line List Cases by Race")
    plt.legend()
    plt.tight_layout()
    
    plt.show()

visualize_state_cases_and_deaths()