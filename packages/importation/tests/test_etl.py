from io import BytesIO
from unittest.mock import ANY, patch

import polars as pl
import pytest
from importation.etl import (
    get_linelist_data,
    get_perkins_et_al_posteriors,
    read_bytes,
)

REPO_DATA_URL = "https://raw.githubusercontent.com/TALexPerkins/sarscov2_unobserved/master/data/"


@pytest.fixture
def mock_urlopen():
    with patch("importation.etl.urlopen") as mock_urlopen:
        yield mock_urlopen


@pytest.fixture
def mock_read_csv():
    with patch("polars.read_csv") as mock_read_csv:
        yield mock_read_csv


def test_read_bytes(mock_urlopen):
    # Mock response data
    mock_response = mock_urlopen.return_value.__enter__.return_value
    mock_response.read.return_value = b"mock data"

    # Call the function
    data_url = REPO_DATA_URL
    file_path = "test.csv"
    result = read_bytes(data_url, file_path)

    # Assertions
    mock_urlopen.assert_called_once_with(f"{data_url}{file_path}", context=ANY)
    assert isinstance(result, BytesIO)


def test_get_perkins_et_al_posteriors(mock_urlopen, mock_read_csv):
    # Mock response data
    mock_response = mock_urlopen.return_value.__enter__.return_value
    mock_response.read.return_value = b"mock csv data"
    mock_read_csv.return_value = pl.DataFrame(
        {
            "Scenario": ["Default", "Other"],
            "rho_travel_alpha": [1, 2],
            "rho_travel_beta": [1, 2],
        }
    )

    # Call the function
    params = get_perkins_et_al_posteriors()

    # Assertions
    assert "rho_travel_alpha" in params.columns
    assert "rho_travel_beta" in params.columns
    assert params.filter(pl.col("Scenario") == "Default").height == 1


def test_get_linelist_data(mock_urlopen, mock_read_csv):
    # Mock response data
    mock_response = mock_urlopen.return_value.__enter__.return_value
    mock_response.read.return_value = b"mock csv data"
    mock_read_csv.return_value = pl.DataFrame(
        {
            "country": ["USA", "USA", "USA", "Other"],
            "summary": ["", "Diamond", "", ""],
            "international_traveler": [1, 1, 0, 1],
            "reporting date": [
                "2020-01-01",
                "2020-01-02",
                "2020-01-03",
                "2020-01-04",
            ],
            "symptom_onset": [
                "2020-01-01",
                "2020-01-02",
                "2020-01-03",
                "2020-01-04",
            ],
            "exposure_start": [
                "2020-01-01",
                "2020-01-02",
                "2020-01-03",
                "2020-01-04",
            ],
            "death": [0, 1, 0, 0],
        },
        schema={
            "country": pl.String,
            "summary": pl.String,
            "international_traveler": pl.Int64,
            "reporting date": pl.Datetime,
            "symptom_onset": pl.Datetime,
            "exposure_start": pl.Datetime,
            "death": pl.Int64,
        },
    )

    # Call the function
    data = get_linelist_data()

    # Assertions
    assert "report_day" in data.columns
    assert "onset_day" in data.columns
    assert "exposure_day" in data.columns
    assert "confirmed_cases" in data.columns
    assert "confirmed_deaths" in data.columns
    assert data.height == 1
