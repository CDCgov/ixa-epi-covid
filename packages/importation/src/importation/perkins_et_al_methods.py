import warnings

import numpy as np
import polars as pl
from scipy.stats import multinomial

import importation.etl


def get_importation_parameter_dict():
    """
    Fetch COVID-19 parameter estimates from GitHub repository and convert to dictionary format for sampling. Perkins et al. 2020 PNAS
    """

    params = importation.etl.get_perkins_et_al_posteriors()

    parameter_dict = {
        "symptomatic_reporting_prob": {
            "type": "beta",
            "alpha": params["rho_travel_alpha"][0],
            "beta": params["rho_travel_beta"][0],
        },
        "case_fatality_ratio": {
            "type": "normal",
            "mean": params["PrDeathSymptom_mean"][0],
            "std": params["PrDeathSymptom_sd"][0],
        },
        "proportion_asymptomatic": {
            "type": "beta",
            "alpha": params["PrAsymptomatic_alpha"][0],
            "beta": params["PrAsymptomatic_beta"][0],
        },
    }

    return parameter_dict


def validate_data(df: pl.DataFrame) -> bool:
    """
    Validate that the input DataFrame contains the required columns for importation parameter calculations.

    Args:
        df (pl.DataFrame): The DataFrame to validate.
    Returns:
        bool: True if the DataFrame is valid, otherwise raises a ValueError with an appropriate error message.
    Raises:
        ValueError:
            If any of the required columns are missing from the DataFrame, or if extra columns are present.
            If the DataFrame is empty.
    """

    required_columns = [
        "confirmed_cases",
        "confirmed_deaths",
        "report_day",
        "onset_day",
        "exposure_day",
        "death",
    ]

    for col in required_columns:
        if col not in df.columns:
            raise ValueError(f"Missing required column: {col}")

    if df.height == 0:
        raise ValueError("Input DataFrame is empty")

    return True


def get_prop_ascf(importation_parameters: dict | pl.DataFrame) -> pl.DataFrame:
    """
    Generate a table of proportions for asymptomatic, symptomatic, and case fatality counts
    over time based on provided importation parameters.
    This function samples parameters from specified distributions to calculate proportions
    for different outcomes (asymptomatic, undetected symptomatic, detected symptomatic, and deaths)
    for a given number of replicates. The proportions are validated to ensure they sum to 1.0.
    This approach assumes that all deaths are detected and that no asymptomatic infections are
    detected or lead to death.
    Args:
        importation_parameters (dict | pl.DataFrame):
            A dictionary or Polars DataFrame containing the parameters
            "proportion_asymptomatic", "case_fatality_ratio", and "symptomatic_reporting_prob".
            If a DataFrame is provided, it must contain these columns and may include a "replicate"
            column if multiple rows are present.
    Returns:
        pl.DataFrame: A DataFrame containing the following columns:
            - proportion_asymptomatic: Proportion of infections that are asymptomatic.
            - case_fatality_ratio: Proportion of symptomatic infections that lead to death.
            - symptomatic_reporting_prob: Reporting probability for imported symptomatic infections.
            - prop_undetected_symptomatic: Proportion of all infections that are symptomatic and remain undetected.
            - prop_detected_symptomatic: Proportion of all infections that are symptomatic and are successfully detected.
            - prop_deaths: Proportion of all infections that lead to death.
    Raises:
        NotImplementedError: If multiple replicates are provided in the input DataFrame.
    """

    # Calculate proportions from sampled parameters for each replicate
    if isinstance(importation_parameters, dict):
        importation_parameters = pl.DataFrame(importation_parameters)

    assert all(
        key in importation_parameters.columns
        for key in [
            "proportion_asymptomatic",
            "case_fatality_ratio",
            "symptomatic_reporting_prob",
        ]
    ), (
        "importation_parameters DataFrame must contain columns: 'proportion_asymptomatic', 'case_fatality_ratio', 'symptomatic_reporting_prob'"
    )

    if importation_parameters.height > 1:
        assert "replicate" in importation_parameters.columns, (
            "importation_parameters DataFrame must contain a 'replicate' column when multiple rows are present"
        )
        raise NotImplementedError(
            "Calculating proportions for multiple replicates is not currently implemented"
        )

    prop_ascf = importation_parameters.with_columns(
        (
            (1 - pl.col("proportion_asymptomatic"))
            * (1 - pl.col("symptomatic_reporting_prob"))
            * (1 - pl.col("case_fatality_ratio"))
        ).alias("prop_undetected_symptomatic"),
        (
            (1 - pl.col("proportion_asymptomatic"))
            * pl.col("symptomatic_reporting_prob")
            * (1 - pl.col("case_fatality_ratio"))
        ).alias("prop_detected_symptomatic"),
        (
            pl.col("case_fatality_ratio")
            * (1 - pl.col("proportion_asymptomatic"))
        ).alias("prop_deaths"),
    )

    # Check that the proportions sum to approximately 1.0
    assert (
        prop_ascf.with_columns(
            (
                pl.col("proportion_asymptomatic")
                + pl.col("prop_undetected_symptomatic")
                + pl.col("prop_detected_symptomatic")
                + pl.col("prop_deaths")
            ).alias("total")
        )
        .filter((pl.col("total") - 1.0).abs() > 1e-10)
        .height
        == 0
    ), "Proportions do not sum to 1"

    return prop_ascf


def prob_undetected_infections(
    n_undetected: int | list,
    known_cases: int,
    known_deaths: int,
    prop_ascf: pl.DataFrame,
) -> pl.DataFrame:
    """
    Calculate the probability of observing known cases and deaths given the number of undetected infections.

    This function computes the multinomial probability of observing the given number of known cases, deaths,
    and undetected infections based on the proportions provided in the `prop_ascf` DataFrame.

    Args:
        n_undetected (int | list): The number of undetected infections. Can be a single integer or a list of integers.
        known_cases (int): The number of known cases.
        known_deaths (int): The number of known deaths.
        prop_ascf (pl.DataFrame): A Polars DataFrame containing the proportions of asymptomatic infections,
            undetected symptomatic infections, detected symptomatic cases, and deaths, along with a replicate identifier.

    Returns:
        pl.DataFrame: A concatenated Polars DataFrame containing the number of undetected infections and the corresponding
        probability of observing each number of undetected cases given the knwon cases and deaths.

    Raises:
        ValueError: If `n_undetected` is not an int, list, or Polars DataFrame.
    """

    if isinstance(n_undetected, int):
        x = [n_undetected, known_cases, known_deaths]
        n = sum(x)
    elif isinstance(n_undetected, list):
        x = [[n, known_cases, known_deaths] for n in n_undetected]
        n = [sum(i) for i in x]
    else:
        raise ValueError(
            "n_undetected must be an int, list, or Polars DataFrame"
        )

    # Calculate multinomial probability of observing n_undetected infections for each row of prop_ascf
    if prop_ascf.height == 1:
        p = [
            prop_ascf["proportion_asymptomatic"][0]
            + prop_ascf["prop_undetected_symptomatic"][0],
            prop_ascf["prop_detected_symptomatic"][0],
            prop_ascf["prop_deaths"][0],
        ]
        pmf_prob = multinomial.pmf(x=x, n=n, p=p)
        prob_data = pl.DataFrame(
            {
                "n_undetected_infections": n_undetected,
                "weight": pmf_prob,
            }
        )
        total_weight = prob_data.select(pl.sum("weight")).item()
        if total_weight < 1e-15:
            warnings.warn(
                "Parameter combination yielded low total probability (p<1e-15) for all undetected infection values. Proceeding"
            )

        if prob_data.select(pl.sum("weight").eq(0)).item():
            return prob_data.with_columns(
                pl.lit(1.0 / prob_data.height).alias("probability")
            )
        else:
            return prob_data.with_columns(
                (pl.col("weight").log() - pl.sum("weight").log())
                .exp()
                .alias("probability")
            )
    else:
        raise ValueError(
            "Calculating the probability of observing n undetected infections given known cases and deaths requires one parameter set in prop_ascf."
        )


def sample_undetected_infections(
    known_cases: int,
    known_deaths: int,
    prop_ascf: pl.DataFrame,
    max_infections: int = 20000,
    seed: int | None = None,
) -> pl.DataFrame:
    """
    Sample undetected infections from a probability distribution based on known cases, known deaths,
    and a given proportion of ascertainment.

    This function generates samples of undetected infections using a probability distribution
    calculated from the provided known cases, known deaths, and ascertainment proportions.
    The sampling is performed for each replicate in the input data.

    Args:
        known_cases (int): The number of known cases.
        known_deaths (int): The number of known deaths.
        prop_ascf (pl.DataFrame): A Polars DataFrame containing the proportion of ascertainment
            for each replicate. It must include a column named "replicate".
        max_infections (int, optional): The maximum number of infections to consider in the
            probability distribution. Defaults to 20000.
        seed (int, optional): A random seed for reproducibility. Defaults to None.

    Returns:
        pl.DataFrame: A Polars DataFrame containing the sampled number of undetected infections
        for each replicate, along with the corresponding replicate information from `prop_ascf`.
    """

    if seed is not None:
        rng = np.random.default_rng(seed)
    else:
        rng = np.random.default_rng()

    prob_data = prob_undetected_infections(
        n_undetected=list(
            range(max_infections + 1 - (known_cases + known_deaths))
        ),
        known_cases=known_cases,
        known_deaths=known_deaths,
        prop_ascf=prop_ascf,
    )

    sampled_undetected = rng.choice(
        prob_data["n_undetected_infections"].to_list(),
        size=1,
        p=prob_data["probability"].to_list(),
    )
    sampled_df = pl.DataFrame(
        {
            "n_undetected_infections": sampled_undetected,
        }
    ).join(prop_ascf, how="cross")

    return sampled_df


def sample_us_importation_incidence_data(
    reporting_data: pl.DataFrame,
    importation_parameters: dict | pl.DataFrame,
    max_infections: int = 20000,
    seed: int | None = None,
) -> pl.DataFrame:
    """
    Create synthetic dataset of the total number of infections by sampling undetected infections.

    The methods in Perkins et al. 2020 bootstrap by building kernel density estimators of the observed data and sampling
    from those distributions. Here, we instead sample rows from the observed data to create synthetic
    datasets of undetected infections. This approach preserves the joint distribution of day,
    onset_day, and exposure_day with limited effort, albeit likely overfitted to the 153 data points.
    """

    prop_ascf = get_prop_ascf(importation_parameters)

    # Sample undetected infections based on known cases and deaths
    synthetic_count_data = sample_undetected_infections(
        known_cases=reporting_data["confirmed_cases"][0],
        known_deaths=reporting_data["confirmed_deaths"][0],
        prop_ascf=prop_ascf,
        max_infections=max_infections,
        seed=seed,
    )

    # Sample day, onset_day, and exposure_day for undetected infections by sampling row from data for each total
    synthetic_count_data = synthetic_count_data.with_columns(
        (
            synthetic_count_data["n_undetected_infections"]
            + reporting_data["confirmed_cases"][0]
            + reporting_data["confirmed_deaths"][0]
        ).alias("total_infections")
    )

    assert synthetic_count_data.height == 1, (
        "Expected one row of synthetic count data for the provided importation_parameters"
    )

    total_infections = synthetic_count_data["total_infections"][0]

    sampled_rows = reporting_data.sample(
        n=total_infections,
        with_replacement=True,
        seed=seed,
    )

    return sampled_rows
