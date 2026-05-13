import os
from pathlib import Path

import polars as pl
from census import Census
from dotenv import load_dotenv
from us import states


def get_api_key() -> str:
    """Get the Census API key from the environment variable"""
    load_dotenv()
    return os.getenv("CENSUS_API_KEY")


def get_total_state_population_data(
    year: int | None = None, cache: bool = True
) -> pl.DataFrame:
    """
    Retrieve state population data from the US Census API.

    This function fetches state population data for a given year using the US Census API.
    If the data for the specified year is cached locally, it will be loaded from the cache.
    Otherwise, the data will be fetched from the API and optionally cached for future use.

    Args:
        year (int | None, optional): The year for which to retrieve population data.
            If None, the latest available data will be fetched. Defaults to None.
        cache (bool, optional): Whether to cache the retrieved data locally.
            Defaults to True.

    Returns:
        pl.DataFrame: A DataFrame containing the state population data with the following columns:
            - "state_name" (str): The name of the state.
            - "state" (int): The state FIPS code.
            - "population" (int): The population of the state.

    Notes:
        - The function uses the Census API and requires a valid API key.
        - Cached files are stored in the ".cache" directory with filenames in the format
          "state_population_data_<year>.csv".
    Raises:
        NotImplementedError: If the year is not 2020 or if caching is disabled, as population data
            is currently only available for the year 2020 with caching.
    """
    if year is None or year != 2020 or cache is False:
        raise NotImplementedError(
            "Population data is currently only avaialble as a cache for the year 2020. "
        )
    filename = f"state_population_data_{year if year else 'latest'}.csv"
    filepath = Path(".cache") / filename
    if filepath.exists() and cache:
        return pl.read_csv(filepath)
    else:
        api_key = get_api_key()
        c = Census(api_key, year=year)
        state_population_data = c.acs5.state(
            ("B01003_001E", "NAME"), Census.ALL
        )
        state_population_df = pl.DataFrame(state_population_data).select(
            [
                pl.col("NAME").alias("state_name"),
                pl.col("state").cast(pl.Int64),
                pl.col("B01003_001E").alias("population").cast(pl.Int64),
            ]
        )
        if cache:
            os.makedirs(filepath.parent, exist_ok=True)
            # Cache the DataFrame for faster future access
            state_population_df.write_csv(filepath)
    return state_population_df


def get_state_proportion_population_data(
    state: str, year: int | None = None, cache: bool = True
) -> float:
    """
    Get the population of a specific state as a proportion of the total US population.

    Args:
        state (str): The name, FIPS code, or abbreviation of the state.
        year (int | None, optional): The year for which to retrieve population data.
            If None, the most recent data is used. Defaults to None.
        cache (bool, optional): Whether to use cached data if available. Defaults to True.

    Returns:
        float: The proportion of the state's population relative to the total US population.

    Raises:
        ValueError: If the provided state name, FIPS code, or abbreviation is invalid.
    """
    state = states.lookup(state)
    if state is None:
        raise ValueError(f"Invalid state name, fips, or abbreviation: {state}")
    state_name = state.name
    state_population_df = get_total_state_population_data(
        year=year, cache=cache
    )
    total_population = state_population_df["population"].sum()
    state_population = state_population_df.filter(
        pl.col("state_name") == state_name
    )["population"].item()
    return state_population / total_population
