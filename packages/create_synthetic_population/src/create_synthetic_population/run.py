import argparse
import math
import os
import time
from pathlib import Path

import geopandas as gpd
import numpy as np
import pandas as pd
import requests
import us
from dotenv import load_dotenv

load_dotenv()

CENSUS_API_KEY = os.environ.get("CENSUS_API_KEY", "")

DEFAULT_INPUT_DIR = "input"
DEFAULT_STATE = "WY"
DEFAULT_SEED = 1234
DEFAULT_SCHOOL_RATIO = 0.0005
DEFAULT_WORKPLACE_PER_CAPITA_RATIO = 0.1
DEFAULT_PUMS_DATA_YEAR = 2023

CROSSWALK_FILE = "2020_Census_Tract_to_2020_PUMA.csv"
CROSSWALK_URL = "https://www2.census.gov/geo/docs/maps-data/data/rel2020/2020_Census_Tract_to_2020_PUMA.txt"


def parse_args(argv=None):
    parser = argparse.ArgumentParser(
        description="Generate a synthetic population for a US state using Census PUMS data.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--state",
        default=DEFAULT_STATE,
        help="US state abbreviation (e.g. WY, CA, TX)",
    )
    parser.add_argument(
        "--size",
        type=lambda s: int(s.replace("_", "")),
        default=1000,
        dest="population_size",
        help="Target synthetic population size (e.g. 1000, 1_000, 10_000)",
    )
    parser.add_argument(
        "--year",
        type=int,
        default=DEFAULT_PUMS_DATA_YEAR,
        help="ACS/PUMS data year",
    )
    parser.add_argument(
        "--input-dir",
        type=Path,
        default=Path(DEFAULT_INPUT_DIR),
        help="Directory for cached input/download files",
    )
    parser.add_argument(
        "--people-filepath",
        type=Path,
        default=None,
        help="Custom output path for people CSV",
    )
    parser.add_argument(
        "--region-filepath",
        type=Path,
        default=None,
        help="Custom output path for region CSV",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=DEFAULT_SEED,
        help="Random seed for reproducibility",
    )
    parser.add_argument(
        "--school-ratio",
        type=float,
        default=DEFAULT_SCHOOL_RATIO,
        help="Schools per capita ratio",
    )
    parser.add_argument(
        "--work-ratio",
        type=float,
        default=DEFAULT_WORKPLACE_PER_CAPITA_RATIO,
        help="Workplaces per capita ratio",
    )
    parser.add_argument(
        "--census-api-key",
        type=str,
        default=None,
        help="Census API key (overrides CENSUS_API_KEY env var)",
    )
    parser.add_argument(
        "--plot",
        action="store_true",
        help="Show a plot of region centroids and tract boundaries",
    )
    return parser.parse_args(argv)


def load_pums(state_synth, state_fips, year_synth, census_api_key, input_dir):
    pums_file = input_dir / f"pums_{state_synth}_{year_synth}.csv"

    pums_variables = [
        "SERIALNO",
        "SPORDER",
        "PWGTP",
        "AGEP",
        "SEX",
        "PUMA",
        "SCH",
        "SCHG",
        "WRK",
        "WGTP",
        "NP",
    ]

    if not pums_file.exists():
        print(f"Downloading PUMS data for {state_synth}...")
        get_vars = ",".join(pums_variables)
        url = (
            f"https://api.census.gov/data/{year_synth}/acs/acs1/pums"
            f"?get={get_vars}"
            f"&for=state:{state_fips}"
            f"&key={census_api_key}"
        )
        resp = requests.get(url)
        resp.raise_for_status()
        data = resp.json()
        headers = data[0]
        rows = data[1:]
        sample_pums = pd.DataFrame(rows, columns=headers)

        for col in ["SPORDER", "PWGTP", "AGEP", "WGTP", "NP"]:
            sample_pums[col] = pd.to_numeric(sample_pums[col], errors="coerce")

        if "state" in sample_pums.columns:
            sample_pums = sample_pums.rename(columns={"state": "STATE"})
        elif "ST" in sample_pums.columns:
            sample_pums = sample_pums.rename(columns={"ST": "STATE"})

        if "STATE" not in sample_pums.columns:
            sample_pums["STATE"] = state_fips

        sample_pums.to_csv(pums_file, index=False)
    else:
        print(f"Reading cached PUMS data from {pums_file}")
        sample_pums = pd.read_csv(
            pums_file,
            dtype={
                "SERIALNO": str,
                "PUMA": str,
                "STATE": str,
                "SCH": str,
                "SCHG": str,
                "WRK": str,
                "SEX": str,
            },
        )

    sample_pums["SERIALNO"] = sample_pums["SERIALNO"].astype(str)
    sample_pums["PUMA"] = sample_pums["PUMA"].astype(str)
    if "STATE" in sample_pums.columns:
        sample_pums["STATE"] = sample_pums["STATE"].astype(str)

    return sample_pums


def load_crosswalk(input_dir):
    tract_puma_crosswalk_file = input_dir / CROSSWALK_FILE
    if not tract_puma_crosswalk_file.exists():
        print("Downloading tract-to-PUMA crosswalk...")
        crosswalk_url = CROSSWALK_URL
        resp = requests.get(crosswalk_url)
        resp.raise_for_status()
        with open(tract_puma_crosswalk_file, "wb") as f:
            f.write(resp.content)

    crosswalk = pd.read_csv(
        tract_puma_crosswalk_file,
        dtype={
            "STATEFP": str,
            "COUNTYFP": str,
            "TRACTCE": str,
            "PUMA5CE": str,
        },
    )
    crosswalk["puma_id"] = crosswalk["STATEFP"] + crosswalk["PUMA5CE"]
    crosswalk["tract_id"] = (
        crosswalk["STATEFP"] + crosswalk["COUNTYFP"] + crosswalk["TRACTCE"]
    )

    tracts_by_puma = (
        crosswalk.groupby("puma_id")["tract_id"].apply(list).reset_index()
    )
    tracts_by_puma.columns = ["puma_id", "tracts"]
    return tracts_by_puma


def load_tracts(state_synth, year_synth):
    print(f"Downloading tract geometries for {state_synth}...")
    import pygris

    tracts_gdf = pygris.tracts(state=state_synth, year=year_synth, cb=True)
    centroids = tracts_gdf.geometry.to_crs(epsg=3857).centroid.to_crs(
        tracts_gdf.crs
    )
    tracts_gdf = tracts_gdf.copy()
    tracts_gdf["lat"] = centroids.y
    tracts_gdf["lon"] = centroids.x
    return tracts_gdf


def create_places(tracts_gdf, n, id_col, rng):
    sample = tracts_gdf.sample(
        n=n, replace=True, random_state=rng.integers(2**31)
    ).reset_index(drop=True)
    return pd.DataFrame(
        {
            id_col: sample["GEOID"]
            + (sample.index + 1).astype(str).str.zfill(6),
            "lat": sample["lat"].values,
            "lon": sample["lon"].values,
            "enrolled": 0,
        }
    )


def sample_population(
    household_pums,
    sample_pums,
    workplace_ids,
    school_ids,
    population_size,
    rng,
):
    start_time = time.time()
    n_households = len(household_pums)

    weights = household_pums["WGTP"].values.astype(float)
    weights = weights / weights.sum()

    # Estimate how many households we need based on average household size
    avg_people_per_hh = len(sample_pums) / n_households
    n_hh_needed = math.ceil(population_size / avg_people_per_hh * 1.05)

    # Sample all households at once with replacement
    sampled_indices = rng.choice(
        n_households, size=n_hh_needed, replace=True, p=weights
    )
    house_sample = (
        household_pums.iloc[sampled_indices].copy().reset_index(drop=True)
    )
    house_sample["house_number"] = range(1, n_hh_needed + 1)

    # Expand to people via merge
    synth_pop_df = house_sample.merge(
        sample_pums, on=["SERIALNO", "WGTP", "NP"], how="left"
    )

    # Trim to target population size
    if len(synth_pop_df) > population_size:
        synth_pop_df = synth_pop_df.iloc[:population_size].copy()

    # Vectorized workplace assignment
    wrk_mask = synth_pop_df["WRK"].astype(str) == "1"
    n_workers = wrk_mask.sum()
    synth_pop_df["workplace_id"] = pd.array(
        [pd.NA] * len(synth_pop_df), dtype="object"
    )
    if n_workers > 0:
        synth_pop_df.loc[wrk_mask, "workplace_id"] = rng.choice(
            workplace_ids, size=n_workers
        )

    # Vectorized school assignment
    sch_mask = synth_pop_df["SCH"].astype(str).isin(["2", "3"])
    n_students = sch_mask.sum()
    synth_pop_df["school_id"] = pd.array(
        [pd.NA] * len(synth_pop_df), dtype="object"
    )
    if n_students > 0:
        synth_pop_df.loc[sch_mask, "school_id"] = rng.choice(
            school_ids, size=n_students
        )

    elapsed = time.time() - start_time
    print(f"Population sampling took {elapsed:.2f}s")
    return synth_pop_df


def assign_geography(synth_pop_df, tracts_by_puma, tracts_gdf, rng):
    house_puma_df = (
        synth_pop_df[["house_number", "STATE", "PUMA"]]
        .drop_duplicates()
        .copy()
    )
    house_puma_df["STATE"] = house_puma_df["STATE"].astype(str)
    house_puma_df["PUMA"] = house_puma_df["PUMA"].astype(str)
    house_puma_df["puma_id"] = house_puma_df["STATE"] + house_puma_df["PUMA"]

    house_puma_df = house_puma_df.merge(
        tracts_by_puma, on="puma_id", how="left"
    )

    house_puma_df["tract_id"] = house_puma_df["tracts"].apply(
        lambda x: rng.choice(x)
        if isinstance(x, list) and len(x) > 0
        else np.nan
    )
    house_puma_df = house_puma_df.drop(columns=["tracts"])

    synth_pop_region_df = synth_pop_df.merge(
        house_puma_df[["house_number", "STATE", "PUMA", "tract_id"]],
        on=["house_number", "STATE", "PUMA"],
        how="left",
    )

    tracts_info = tracts_gdf[
        ["GEOID", "COUNTYFP", "TRACTCE", "lat", "lon"]
    ].copy()
    if isinstance(tracts_info, gpd.GeoDataFrame):
        tracts_info = pd.DataFrame(tracts_info.drop(columns="geometry"))

    synth_pop_region_df = synth_pop_region_df.merge(
        tracts_info,
        left_on="tract_id",
        right_on="GEOID",
        how="left",
        suffixes=("", "_tract"),
    )

    synth_pop_region_df["home_id"] = synth_pop_region_df["tract_id"].astype(
        str
    ) + synth_pop_region_df["house_number"].apply(lambda x: f"{x:06d}")

    return synth_pop_region_df


def build_outputs(synth_pop_region_df):
    people_df = synth_pop_region_df[
        ["AGEP", "home_id", "school_id", "workplace_id"]
    ].rename(
        columns={
            "AGEP": "age",
            "home_id": "homeId",
            "school_id": "schoolId",
            "workplace_id": "workplaceId",
        }
    )

    region_df = (
        synth_pop_region_df[["tract_id", "lat", "lon"]]
        .rename(columns={"tract_id": "censusTractId"})
        .drop_duplicates()
    )

    return people_df, region_df


def run(argv=None):
    args = parse_args(argv)

    state_synth = args.state
    population_size = args.population_size
    year_synth = args.year
    school_per_pop_ratio = args.school_ratio
    work_per_pop_ratio = args.work_ratio

    rng = np.random.default_rng(args.seed)

    census_api_key = args.census_api_key or CENSUS_API_KEY

    input_dir = args.input_dir
    input_dir.mkdir(parents=True, exist_ok=True)

    size_str = f"{population_size:_}"
    people_file = args.people_filepath or (
        input_dir / f"synth_pop_people_{state_synth}_{size_str}.csv"
    )
    region_file = args.region_filepath or (
        input_dir / f"synth_pop_region_{state_synth}_{size_str}.csv"
    )
    people_file.parent.mkdir(parents=True, exist_ok=True)
    region_file.parent.mkdir(parents=True, exist_ok=True)

    state_obj = us.states.lookup(state_synth)
    if state_obj is None:
        raise ValueError(f"Unknown state: {state_synth}")
    state_fips = state_obj.fips

    sample_pums = load_pums(
        state_synth, state_fips, year_synth, census_api_key, input_dir
    )
    household_pums = (
        sample_pums[["SERIALNO", "WGTP", "NP"]]
        .drop_duplicates()
        .reset_index(drop=True)
    )

    tracts_by_puma = load_crosswalk(input_dir)
    tracts_gdf = load_tracts(state_synth, year_synth)

    n_schools = math.ceil(school_per_pop_ratio * population_size)
    n_workplaces = math.ceil(work_per_pop_ratio * population_size)

    synth_school_df = create_places(tracts_gdf, n_schools, "school_id", rng)
    synth_workplace_df = create_places(
        tracts_gdf, n_workplaces, "workplace_id", rng
    )

    synth_pop_df = sample_population(
        household_pums,
        sample_pums,
        synth_workplace_df["workplace_id"].values,
        synth_school_df["school_id"].values,
        population_size,
        rng,
    )

    synth_pop_region_df = assign_geography(
        synth_pop_df, tracts_by_puma, tracts_gdf, rng
    )
    people_df, region_df = build_outputs(synth_pop_region_df)

    region_df.to_csv(region_file, index=False, na_rep="")
    people_df.to_csv(people_file, index=False, na_rep="")

    print(f"Wrote {len(people_df)} people to {people_file}")
    print(f"Wrote {len(region_df)} regions to {region_file}")

    if args.plot:
        import matplotlib.pyplot as plt

        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))
        region_df.plot.scatter(x="lon", y="lat", ax=ax1, s=5)
        ax1.set_title("Region centroids")
        tracts_gdf.boundary.plot(ax=ax2, linewidth=0.3)
        ax2.set_title("Tract boundaries")
        ax2.axis("off")
        plt.tight_layout()
        plt.show()

    return people_file, region_file


if __name__ == "__main__":
    run()
