import polars as pl
import pytest
from importation.etl import get_linelist_data
from importation.model import (
    ImportationModel,
    RegionalModel,
    summarize_linelist_importation_data,
)


@pytest.fixture
def test_parameters():
    return {
        "symptomatic_reporting_prob": 0.5,
        "case_fatality_ratio": 0.01,
        "proportion_asymptomatic": 0.7,
    }


def test_summarize_linelist_importation_data(test_parameters):
    data = pl.DataFrame(
        {
            "report_day": [1, 1, 1, 2, 2, 2, 4, 4, 4],
        }
    )
    summarized_data = summarize_linelist_importation_data(
        data, report_day_bounds=(1, 6)
    )
    assert isinstance(summarized_data, pl.DataFrame)
    assert "time" in summarized_data.columns
    assert "imported_infections" in summarized_data.columns
    assert summarized_data.sort("time")["time"].to_list() == [1, 2, 3, 4, 5]
    assert summarized_data["imported_infections"].to_list() == [3, 3, 0, 3, 0]

    summarized_data = summarize_linelist_importation_data(
        data, report_day_bounds=(1, 6), expand=False
    )
    assert summarized_data.sort("time")["time"].to_list() == [1, 2, 4]
    assert summarized_data["imported_infections"].to_list() == [3, 3, 3]


def test_importation_model_initialization(test_parameters):
    model = ImportationModel(
        data=get_linelist_data(),
        parameters=test_parameters,
        state_model="proportional",
        national_model="multinomial",
    )
    assert model.national_model_type == "multinomial"
    assert model.state_model_type == "proportional"
    assert isinstance(model.national_model, RegionalModel)
    assert isinstance(model.state_model, RegionalModel)


def test_importation_model_without_national_model(test_parameters):
    model = ImportationModel(
        data=get_linelist_data(),
        parameters=test_parameters,
        state_model="multinomial",
    )
    assert model.national_model is None
    assert model.state_model_type == "multinomial"


def test_importation_model_without_national_model_fail_proportional(
    test_parameters,
):
    # Test that providing a proportional state model without a national model raises an error
    with pytest.raises(AssertionError):
        ImportationModel(
            data=get_linelist_data(),
            parameters=test_parameters,
            state_model="proportional",
        )


def test_importation_model_without_national_model_fail_data(test_parameters):
    # Test that invalid data raises a ValueError
    with pytest.raises(ValueError):
        ImportationModel(
            data=get_linelist_data().drop("report_day"),
            parameters=test_parameters,
            national_model="multinomial",
            state_model="proportional",
        )


def test_importation_model_with_national_model_fail_model_type(
    test_parameters,
):
    # Test that providing an invalid model type raises an error
    with pytest.raises(AssertionError):
        ImportationModel(
            data=get_linelist_data(),
            parameters=test_parameters,
            national_model="invalid_model",
            state_model="proportional",
        )


def test_importation_model_with_national_model_fail_multinomial_state(
    test_parameters,
):
    # Test that providing a multinomial state model with a national model raises an error
    with pytest.raises(AssertionError):
        ImportationModel(
            data=get_linelist_data(),
            parameters=test_parameters,
            national_model="multinomial",
            state_model="multinomial",
        )


def test_sample_state_importation_incidence(test_parameters):
    model = ImportationModel(
        data=get_linelist_data(),
        parameters=test_parameters,
        state_model="proportional",
        national_model="multinomial",
    )
    result = model.sample_state_importation_incidence(proportion=0.5)
    assert isinstance(result, pl.DataFrame)
    assert "time" in result.columns
    assert "imported_infections" in result.columns
    assert model.parameters["proportion"] == 0.5


def test_sample_state_importation_incidence_missing_proportion(
    test_parameters,
):
    model = ImportationModel(
        data=get_linelist_data(),
        parameters=test_parameters,
        state_model="proportional",
        national_model="multinomial",
    )
    with pytest.raises(ValueError):
        model.sample_state_importation_incidence()


def test_sample_state_importation_incidence_state_proportion(test_parameters):
    model = ImportationModel(
        data=get_linelist_data(),
        parameters=test_parameters,
        state_model="proportional",
        national_model="multinomial",
    )
    result = model.sample_state_importation_incidence(
        state="Alabama", year=2020
    )
    assert isinstance(result, pl.DataFrame)
    assert "time" in result.columns
    assert "imported_infections" in result.columns
    assert "proportion" in model.parameters
