from unittest.mock import patch

import polars as pl
import pytest
from importation.perkins_et_al_methods import (
    get_importation_parameter_dict,
    get_prop_ascf,
    prob_undetected_infections,
    sample_undetected_infections,
)


@pytest.fixture
def mock_parameters():
    with patch("importation.etl.get_parameters") as mock_get_parameters:
        mock_get_parameters.return_value = {
            "PrAsymptomatic_alpha": [2],
            "PrAsymptomatic_beta": [5],
            "PrDeathSymptom_mean": [0.02],
            "PrDeathSymptom_sd": [0.005],
            "rho_travel_alpha": [3],
            "rho_travel_beta": [7],
        }
        yield mock_get_parameters


@pytest.fixture
def dummy_values():
    return {
        "symptomatic_reporting_prob": 0.3,
        "case_fatality_ratio": 0.02,
        "proportion_asymptomatic": 0.5,
    }


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
