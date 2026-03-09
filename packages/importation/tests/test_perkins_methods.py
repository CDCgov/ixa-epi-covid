from unittest.mock import patch

import polars as pl
import pytest
from importation.perkins_et_al_methods import (
    get_importation_parameter_dict,
    get_prop_ascf,
    prob_undetected_infections,
    sample_undetected_infections,
    sample_us_importation_incidence_data,
)


@pytest.fixture
def mock_parameters():
    with patch(
        "importation.etl.get_perkins_et_al_posteriors"
    ) as mock_get_perkins_et_al_posteriors:
        mock_get_perkins_et_al_posteriors.return_value = {
            "PrAsymptomatic_alpha": [2],
            "PrAsymptomatic_beta": [5],
            "PrDeathSymptom_mean": [0.02],
            "PrDeathSymptom_sd": [0.005],
            "rho_travel_alpha": [3],
            "rho_travel_beta": [7],
        }
        yield mock_get_perkins_et_al_posteriors


@pytest.fixture
def dummy_values():
    return {
        "symptomatic_reporting_prob": 0.3,
        "case_fatality_ratio": 0.02,
        "proportion_asymptomatic": 0.5,
    }


def test_validate_data():
    from importation.perkins_et_al_methods import validate_data

    # Create a valid DataFrame
    valid_data = pl.DataFrame(
        {
            "confirmed_cases": [10, 20, 30],
            "confirmed_deaths": [1, 2, 3],
            "max_infections": [100, 200, 300],
            "report_day": [1, 2, 3],
            "onset_day": [1, 2, 3],
            "exposure_day": [1, 2, 3],
            "death": [0, 1, 0],
        }
    )
    try:
        validate_data(valid_data)
    except ValueError:
        pytest.fail(
            "validate_data raised ValueError unexpectedly for valid data!"
        )

    # Create an invalid DataFrame (missing 'report_day' column)
    invalid_data = valid_data.drop("report_day")
    with pytest.raises(ValueError):
        validate_data(invalid_data)

    # Create an invalid DataFrame (missing 'imported_infections' column)
    invalid_data_2 = valid_data.filter(
        pl.col("report_day") == 10_000
    )  # This will create an empty DataFrame
    assert invalid_data_2.height == 0, "Expected invalid_data_2 to be empty"
    with pytest.raises(ValueError):
        validate_data(invalid_data_2)


def test_get_importation_parameter_dict(mock_parameters):
    parameter_dict = get_importation_parameter_dict()

    assert isinstance(parameter_dict, dict)
    assert all(
        key in parameter_dict
        for key in [
            "symptomatic_reporting_prob",
            "case_fatality_ratio",
            "proportion_asymptomatic",
        ]
    )
    assert parameter_dict["symptomatic_reporting_prob"]["type"] == "beta"
    assert parameter_dict["case_fatality_ratio"]["type"] == "normal"
    assert parameter_dict["proportion_asymptomatic"]["type"] == "beta"
    assert parameter_dict["symptomatic_reporting_prob"]["alpha"] == 3
    assert parameter_dict["symptomatic_reporting_prob"]["beta"] == 7
    assert parameter_dict["case_fatality_ratio"]["mean"] == 0.02
    assert parameter_dict["case_fatality_ratio"]["std"] == 0.005
    assert parameter_dict["proportion_asymptomatic"]["alpha"] == 2
    assert parameter_dict["proportion_asymptomatic"]["beta"] == 5


def test_get_prop_ascf(dummy_values):
    prop_ascf = get_prop_ascf(importation_parameters=dummy_values)

    assert prop_ascf.shape == (1, 6)
    assert all(
        col in prop_ascf.columns
        for col in [
            "proportion_asymptomatic",
            "case_fatality_ratio",
            "symptomatic_reporting_prob",
            "prop_undetected_symptomatic",
            "prop_detected_symptomatic",
            "prop_deaths",
        ]
    )

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
    )


def test_prob_undetected_infections(dummy_values):
    prop_ascf = get_prop_ascf(importation_parameters=dummy_values)
    n_undetected = 10
    known_cases = 5
    known_deaths = 2

    prob_data = prob_undetected_infections(
        n_undetected, known_cases, known_deaths, prop_ascf
    )

    assert prob_data.shape[0] == 1
    assert all(
        col in prob_data.columns
        for col in ["n_undetected_infections", "probability"]
    )


def test_prob_undetected_infections_list(dummy_values):
    prop_ascf = get_prop_ascf(importation_parameters=dummy_values)
    n_undetected = [0, 1, 2, 3, 4]
    known_cases = 5
    known_deaths = 2

    prob_data = prob_undetected_infections(
        n_undetected, known_cases, known_deaths, prop_ascf
    )

    assert prob_data.shape[0] == len(n_undetected)
    assert all(
        col in prob_data.columns
        for col in ["n_undetected_infections", "probability"]
    )


def test_prob_undetected_infections_zero_probability_value(dummy_values):
    prop_ascf = get_prop_ascf(importation_parameters=dummy_values)
    known_cases = 500
    known_deaths = 2
    n_undetected = 1

    prob_data = prob_undetected_infections(
        n_undetected, known_cases, known_deaths, prop_ascf
    )

    assert prob_data.shape[0] == 1
    assert all(
        col in prob_data.columns
        for col in ["n_undetected_infections", "probability"]
    )
    assert prob_data.item(0, "probability") == 1.0
    assert prob_data.item(0, "weight") == 0.0


def test_prob_undetected_infections_rounding_zero_probability_list(
    dummy_values,
):
    # Expect a large number of undetected, such that a low number of undetected is prob 0 but a slightly higher number of undetected p > 0 and p << 1e-6
    prop_ascf = get_prop_ascf(
        importation_parameters={
            "symptomatic_reporting_prob": 0.002,
            "case_fatality_ratio": 0.002,
            "proportion_asymptomatic": 0.99,
        }
    )
    known_cases = 100
    known_deaths = 2
    n_undetected = list(range(20_000))

    prob_data = prob_undetected_infections(
        n_undetected, known_cases, known_deaths, prop_ascf
    )

    print(prob_data)

    assert prob_data.shape[0] == 20_000
    assert all(
        col in prob_data.columns
        for col in ["n_undetected_infections", "probability"]
    )

    zero_undetected_infections_info = prob_data.filter(
        pl.col("n_undetected_infections") == 0
    )
    max_undetected_infections_info = prob_data.filter(
        pl.col("n_undetected_infections") == pl.max("n_undetected_infections")
    )

    assert zero_undetected_infections_info.select("probability").item() == 0.0
    assert max_undetected_infections_info.select("probability").item() > 0.0

    assert zero_undetected_infections_info.select("weight").item() == 0.0
    assert max_undetected_infections_info.select("weight").item() > 0.0
    assert max_undetected_infections_info.select("weight").item() < 1e-12

    assert prob_data.select(pl.sum("weight")).item() < 1e-12
    assert prob_data.select(pl.sum("probability")).item() == pytest.approx(1.0)


def test_sample_undetected_infections(dummy_values):
    prop_ascf = get_prop_ascf(importation_parameters=dummy_values)
    known_cases = 5
    known_deaths = 2
    max_infections = 100
    seed = 42

    sampled_data = sample_undetected_infections(
        known_cases, known_deaths, prop_ascf, max_infections, seed
    )

    assert sampled_data.shape[0] == 1
    assert all(
        col in sampled_data.columns for col in ["n_undetected_infections"]
    )


def test_sample_undetected_infections_zero_handling(dummy_values):
    prop_ascf = get_prop_ascf(importation_parameters=dummy_values)
    known_cases = 500
    known_deaths = 2
    max_infections = 502
    seed = 42

    sampled_data = sample_undetected_infections(
        known_cases, known_deaths, prop_ascf, max_infections, seed
    )

    assert sampled_data.shape[0] == 1
    assert all(
        col in sampled_data.columns for col in ["n_undetected_infections"]
    )


@pytest.fixture
def mock_sample_undetected_infections(dummy_values):
    with patch(
        "importation.perkins_et_al_methods.sample_undetected_infections"
    ) as mock_sample:
        mock_sample.return_value = pl.DataFrame(
            {"n_undetected_infections": [10]}
        )
        yield mock_sample


def test_sample_us_importation_incidence_data(
    mock_sample_undetected_infections, dummy_values
):
    reporting_data = pl.DataFrame(
        {"confirmed_cases": [3], "confirmed_deaths": [1]}
    )
    incidence_data = sample_us_importation_incidence_data(
        reporting_data=reporting_data,
        importation_parameters=dummy_values,
        seed=1234,
    )
    assert (
        incidence_data.height == 10 + 3 + 1
    )  # mock_undetected_infections + known_cases + known_deaths
