import os
import ssl
from io import BytesIO
from pathlib import Path
from urllib.request import urlopen

import polars as pl

REPO_DATA_URL = "https://raw.githubusercontent.com/TALexPerkins/sarscov2_unobserved/master/data/"
PACKAGE_DATA_DIR = Path(__file__).resolve().parent / "data"

# =========================== #
# Data Retrieval Functions  #
# =========================== #


def read_bytes(data_url: str, file_path: str) -> pl.DataFrame:
    """
    Read CSV data from a GitHub URL into a Polars DataFrame.
    Args:
        data_url (str): The base URL of the GitHub repository where the data is stored.
        file_path (str): The path to the specific CSV file within the repository.
    Returns:
        pl.DataFrame: A Polars DataFrame containing the data from the specified CSV file.
    """
    if data_url.endswith("/"):
        url = data_url[:-1]
    else:
        url = data_url
    url = f"{url}/{file_path}"
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.load_default_certs()
    with urlopen(url, context=context) as response:
        data = BytesIO(response.read())

    return data


def _packaged_data_path(filename: str) -> Path | None:
    candidate = PACKAGE_DATA_DIR / filename
    if candidate.exists():
        return candidate
    return None


def get_perkins_et_al_posteriors(
    base_filename: str = "perkins_et_al_importation_parameters.csv",
    input_dir: str | Path = "./.cache",
    cache: bool = True,
    scenario: str = "Default",
) -> pl.DataFrame:
    """
    Fetch COVID-19 parameter estimates from GitHub repository. Perkins et al. 2020 PNAS
    Args:
        base_filename (str): The base filename for the parameter estimates CSV file. Defaults to "perkins_et_al_importation_parameters.csv".
        input_dir (str | Path): The directory where cached files are stored. Defaults to "./.cache".
        cache (bool): Whether to cache the retrieved data locally. Defaults to True.
        scenario (str): The scenario name for which to retrieve parameter estimates. This string must be available within the column for "scenario" in the dataset. Defaults to "Default".
    Returns:
        pl.DataFrame: A Polars DataFrame containing the parameter estimates for the specified scenario, with columns corresponding to the parameters and their values.
    """
    filename = f"{scenario.lower()}_{base_filename}"
    # Create cache directory if it doesn't exist
    os.makedirs(input_dir, exist_ok=True)
    if os.path.exists(os.path.join(input_dir, filename)):
        params = pl.read_csv(os.path.join(input_dir, filename))
    else:
        # Load repo output from url
        url = REPO_DATA_URL
        params_file = "sensitivity/covid_params_estimate.csv"

        # Get default scenario parameter estimates
        data = read_bytes(url, params_file)
        params = pl.read_csv(data).filter(pl.col("Scenario") == "Default")
        if cache:
            params.write_csv(os.path.join(input_dir, filename))

    # In lieu of posterior rho_travel estimates, use beta distribution parameters
    # E[rho_travel] = 0.5, SD[rho_travel] \approx 0.2
    params = params.with_columns(
        pl.lit(3).alias("rho_travel_alpha"), pl.lit(3).alias("rho_travel_beta")
    )

    return params


def get_linelist_data(
    filename: str = "raw_perkins_et_al_importation_data.csv",
    input_dir: str | Path = "./.cache",
    cache: bool = True,
    url: str = REPO_DATA_URL,
    linelist_file: str = "2020_03_12_1800EST_linelist_NIHFogarty.csv",
) -> pl.DataFrame:
    """
    Fetch COVID-19 linelist data for analysis from Perkins et al. 2020 PNAS.
    Args:
        filename (str): The filename for the linelist data CSV file. Defaults to "raw_perkins_et_al_importation_data.csv".
        input_dir (str | Path): The directory where cached files are stored. Defaults to "./.cache".
        cache (bool): Whether to cache the retrieved data locally. Defaults to True.
        url (str): The base URL of the GitHub repository where the data is stored. Defaults to REPO_DATA_URL.
        linelist_file (str): The specific filename of the linelist data within the repository. Defaults to "2020_03_12_1800EST_linelist_NIHFogarty.csv".
    Returns:
        pl.DataFrame: A Polars DataFrame containing the linelist data, with columns corresponding to the relevant fields such as "report_day", "onset_day", "exposure_day".
    Notes:
        - The function checks for a cached version of the linelist data in the specified input directory
        - The data is filtered following the methods in Perkins et al. 2020 PNAS to include only US importations that are not associated with the Diamond Princess cruise ship and are marked as international travelers.
    """

    packaged_file = _packaged_data_path(filename)
    if packaged_file is not None:
        linelist_data = pl.read_csv(
            packaged_file,
            schema_overrides={
                "age": pl.Float64,
                "international_traveler": pl.Int64,
                "reporting date": pl.Datetime,
                "symptom_onset": pl.Datetime,
                "exposure_start": pl.Datetime,
            },
        )
    else:
        os.makedirs(input_dir, exist_ok=True)
        cached_file = Path(input_dir) / filename
        if cached_file.exists():
            linelist_data = pl.read_csv(
                cached_file,
                schema_overrides={
                    "age": pl.Float64,
                    "international_traveler": pl.Int64,
                    "reporting date": pl.Datetime,
                    "symptom_onset": pl.Datetime,
                    "exposure_start": pl.Datetime,
                },
            )
        else:  # download repo output from url
            linelist_bytes = read_bytes(url, linelist_file)
            linelist_data = pl.read_csv(
                linelist_bytes,
                null_values="NA",
                schema_overrides={
                    "age": pl.Float64,
                    "international_traveler": pl.Int64,
                    "reporting date": pl.Datetime,
                    "symptom_onset": pl.Datetime,
                    "exposure_start": pl.Datetime,
                },
            )
            if cache:
                linelist_data.write_csv(cached_file)

    us_imports = (
        linelist_data.filter(
            pl.col("country") == "USA",
            ~pl.col("summary").str.contains_any(["iamond"]),
            pl.col("international_traveler"),
        )
        .with_columns(
            (
                pl.col("reporting date")
                - pl.lit("2019-12-31T00:00:00").cast(pl.Datetime)
            ).alias("report_day"),
            (
                pl.col("symptom_onset")
                - pl.lit("2019-12-31T00:00:00").cast(pl.Datetime)
            ).alias("onset_day"),
            (
                pl.col("exposure_start")
                - pl.lit("2019-12-31T00:00:00").cast(pl.Datetime)
            ).alias("exposure_day"),
        )
        .select(
            [
                pl.col("report_day").dt.total_days().cast(pl.Int64),
                pl.col("onset_day").dt.total_days().cast(pl.Int64),
                pl.col("exposure_day").dt.total_days().cast(pl.Int64),
                pl.col("death").fill_null(0).cast(pl.Int64),
            ]
        )
    )

    total_imports = us_imports.height
    total_deaths = us_imports.filter(pl.col("death") != 0).height
    total_cases = total_imports - total_deaths

    summary_data = pl.DataFrame(
        {
            "confirmed_cases": total_cases,
            "confirmed_deaths": total_deaths,
        }
    )

    data = us_imports.join(summary_data, how="cross")

    return data
