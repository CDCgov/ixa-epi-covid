import warnings
from pathlib import Path
from unittest.mock import patch

import geopandas as gpd
import numpy as np
import pandas as pd
import pytest
import requests
from create_synthetic_population import (
    assign_geography,
    build_outputs,
    create_places,
    load_tracts,
    parse_args,
    sample_population,
)
from create_synthetic_population.run import load_pums
from shapely.geometry import Point, Polygon


@pytest.fixture
def rng():
    return np.random.default_rng(42)


@pytest.fixture
def tracts_gdf():
    data = {
        "GEOID": ["56001000100", "56001000200", "56003000300"],
        "STATEFP": ["56", "56", "56"],
        "COUNTYFP": ["001", "001", "003"],
        "TRACTCE": ["000100", "000200", "000300"],
        "geometry": [
            Point(-106.3, 44.8),
            Point(-106.4, 44.9),
            Point(-106.5, 44.7),
        ],
    }
    gdf = gpd.GeoDataFrame(data, crs="EPSG:4326")
    centroids = gdf.geometry.centroid
    gdf["lat"] = centroids.y
    gdf["lon"] = centroids.x
    return gdf


@pytest.fixture
def sample_pums():
    return pd.DataFrame(
        {
            "SERIALNO": ["HU001", "HU001", "HU002", "HU002", "HU003"],
            "SPORDER": [1, 2, 1, 2, 1],
            "PWGTP": [10, 10, 20, 20, 15],
            "AGEP": [35, 10, 42, 8, 55],
            "SEX": ["1", "2", "1", "2", "1"],
            "PUMA": ["00100", "00100", "00100", "00100", "00100"],
            "SCH": ["1", "2", "1", "3", "1"],
            "SCHG": ["0", "04", "0", "02", "0"],
            "WRK": ["1", "0", "1", "0", "1"],
            "WGTP": [50, 50, 60, 60, 40],
            "NP": [2, 2, 2, 2, 1],
            "STATE": ["56", "56", "56", "56", "56"],
        }
    )


@pytest.fixture
def household_pums(sample_pums):
    return (
        sample_pums[["SERIALNO", "WGTP", "NP"]]
        .drop_duplicates()
        .reset_index(drop=True)
    )


@pytest.fixture
def tracts_by_puma():
    return pd.DataFrame(
        {
            "puma_id": ["5600100"],
            "tracts": [["56001000100", "56001000200", "56003000300"]],
        }
    )


WORKPLACE_IDS = np.array(["wp001"])
SCHOOL_IDS = np.array(["sc001"])


@pytest.fixture
def pop_df(household_pums, sample_pums, rng):
    return sample_population(
        household_pums, sample_pums, WORKPLACE_IDS, SCHOOL_IDS, 5, rng
    )


@pytest.fixture
def geo_df(pop_df, tracts_by_puma, tracts_gdf, rng):
    return assign_geography(pop_df, tracts_by_puma, tracts_gdf, rng)


class TestParseArgs:
    def test_defaults(self):
        args = parse_args([])
        assert args.state == "WY"
        assert args.population_size == 1000
        assert args.year == 2023
        assert args.seed == 1234
        assert args.plot is False
        assert args.people_filepath is None
        assert args.region_filepath is None

    def test_named_args(self):
        args = parse_args(["--state", "CA", "--size", "5000"])
        assert args.state == "CA"
        assert args.population_size == 5000

    def test_underscore_population_size(self):
        assert parse_args(["--size", "10_000"]).population_size == 10000
        assert parse_args(["--size", "1_000_000"]).population_size == 1000000

    def test_all_options(self):
        args = parse_args(
            [
                "--state",
                "TX",
                "--size",
                "2000",
                "--year",
                "2022",
                "--input-dir",
                "/tmp/in",
                "--people-filepath",
                "/tmp/out/people.csv",
                "--region-filepath",
                "/tmp/out/region.csv",
                "--seed",
                "99",
                "--school-ratio",
                "0.001",
                "--work-ratio",
                "0.2",
                "--census-api-key",
                "abc123",
                "--plot",
            ]
        )
        assert args.state == "TX"
        assert args.population_size == 2000
        assert args.year == 2022
        assert args.input_dir == Path("/tmp/in")
        assert args.people_filepath == Path("/tmp/out/people.csv")
        assert args.region_filepath == Path("/tmp/out/region.csv")
        assert args.seed == 99
        assert args.school_ratio == 0.001
        assert args.work_ratio == 0.2
        assert args.census_api_key == "abc123"  # pragma: allowlist secret
        assert args.plot is True


class TestLoadTracts:
    def test_no_geographic_crs_warning(self):
        fake_gdf = gpd.GeoDataFrame(
            {
                "GEOID": ["06001000100"],
                "geometry": [
                    Polygon(
                        [
                            (-122.0, 37.0),
                            (-122.1, 37.0),
                            (-122.1, 37.1),
                            (-122.0, 37.1),
                        ]
                    )
                ],
            },
            crs="EPSG:4326",
        )
        with patch("pygris.tracts", return_value=fake_gdf):
            with warnings.catch_warnings():
                warnings.filterwarnings(
                    "error", message="Geometry is in a geographic CRS"
                )
                result = load_tracts("CA", 2023)

        assert "lat" in result.columns and "lon" in result.columns
        assert result["lat"].notna().all() and result["lon"].notna().all()


class TestLoadPums:
    def test_non_json_response_raises_http_error(self, tmp_path):
        class FakeResponse:
            text = """
            <html>
                <body>A valid key must be included.</body>
            </html>
            """

            def raise_for_status(self):
                return None

            def json(self):
                raise requests.exceptions.JSONDecodeError(
                    "Expecting value", self.text, 0
                )

        with patch("requests.get", return_value=FakeResponse()):
            with pytest.raises(requests.HTTPError, match="not JSON"):
                load_pums("IN", "18", 2020, "", tmp_path)


class TestCreatePlaces:
    def test_creates_correct_number(self, tracts_gdf, rng):
        df = create_places(tracts_gdf, 5, "school_id", rng)
        assert len(df) == 5
        assert list(df.columns) == ["school_id", "lat", "lon", "enrolled"]
        assert (df["enrolled"] == 0).all()

    def test_ids_are_unique(self, tracts_gdf, rng):
        assert create_places(tracts_gdf, 10, "workplace_id", rng)[
            "workplace_id"
        ].is_unique

    def test_lat_lon_populated(self, tracts_gdf, rng):
        df = create_places(tracts_gdf, 3, "school_id", rng)
        assert df["lat"].notna().all() and df["lon"].notna().all()

    def test_school_ids_reset_within_tract(self, tracts_gdf, rng):
        sampled = tracts_gdf.iloc[[0, 1, 0, 1]].copy().reset_index(drop=True)
        with patch.object(
            gpd.GeoDataFrame,
            "sample",
            autospec=True,
            return_value=sampled,
        ):
            df = create_places(tracts_gdf, 4, "school_id", rng)

        assert df["school_id"].tolist() == [
            "56001000100001",
            "56001000200001",
            "56001000100002",
            "56001000200002",
        ]

    def test_workplace_ids_reset_within_tract(self, tracts_gdf, rng):
        sampled = tracts_gdf.iloc[[0, 1, 0, 1]].copy().reset_index(drop=True)
        with patch.object(
            gpd.GeoDataFrame,
            "sample",
            autospec=True,
            return_value=sampled,
        ):
            df = create_places(tracts_gdf, 4, "workplace_id", rng)

        assert df["workplace_id"].tolist() == [
            "5600100010000001",
            "5600100020000001",
            "5600100010000002",
            "5600100020000002",
        ]

    @pytest.mark.parametrize(
        ("id_col", "setting_type"),
        [("school_id", "school"), ("workplace_id", "workplace")],
    )
    def test_place_id_overflow_raises(
        self, tracts_gdf, rng, id_col, setting_type
    ):
        sampled = (
            tracts_gdf.iloc[np.zeros(32_768, dtype=int)]
            .copy()
            .reset_index(drop=True)
        )
        with patch.object(
            gpd.GeoDataFrame,
            "sample",
            autospec=True,
            return_value=sampled,
        ):
            with pytest.raises(
                ValueError,
                match=(
                    rf"{setting_type} sequence overflow for tract 56001000100: "
                    r"maximum sequence 32768 exceeds FIPSCode limit 32767"
                ),
            ):
                create_places(tracts_gdf, 32_768, id_col, rng)


class TestSamplePopulation:
    def test_produces_at_least_target(self, household_pums, sample_pums, rng):
        wp = np.array(["wp001", "wp002"])
        sc = np.array(["sc001", "sc002"])
        df = sample_population(household_pums, sample_pums, wp, sc, 5, rng)
        assert len(df) >= 5

    def test_workers_get_workplaces(self, pop_df):
        workers = pop_df[pop_df["WRK"].astype(str) == "1"]
        assert workers["workplace_id"].notna().all()

    def test_students_get_schools(self, pop_df):
        students = pop_df[pop_df["SCH"].astype(str).isin(["2", "3"])]
        if len(students) > 0:
            assert students["school_id"].notna().all()

    def test_non_workers_no_workplace(self, pop_df):
        non_workers = pop_df[pop_df["WRK"].astype(str) != "1"]
        if len(non_workers) > 0:
            assert non_workers["workplace_id"].isna().all()


class TestAssignGeography:
    def test_adds_tract_and_home_id(self, geo_df):
        assert "home_id" in geo_df.columns
        assert "tract_id" in geo_df.columns
        assert geo_df["home_id"].notna().all()

    def test_home_ids_reset_within_tract_and_households_share_home(
        self, tracts_gdf, rng
    ):
        synth_pop_df = pd.DataFrame(
            {
                "person_id": [1, 2, 3, 4, 5],
                "house_number": [1, 1, 2, 3, 3],
                "STATE": ["56"] * 5,
                "PUMA": ["00100", "00100", "00200", "00300", "00300"],
            }
        )
        tracts_by_puma = pd.DataFrame(
            {
                "puma_id": ["5600100", "5600200", "5600300"],
                "tracts": [
                    ["56001000100"],
                    ["56001000200"],
                    ["56001000100"],
                ],
            }
        )

        geo_df = assign_geography(
            synth_pop_df, tracts_by_puma, tracts_gdf, rng
        )
        households = (
            geo_df[["house_number", "home_id"]]
            .drop_duplicates()
            .sort_values("house_number")
        )

        assert households["home_id"].tolist() == [
            "560010001000001",
            "560010002000001",
            "560010001000002",
        ]
        assert geo_df.groupby("house_number")["home_id"].nunique().eq(1).all()

    def test_home_id_overflow_raises(self, tracts_gdf, rng):
        synth_pop_df = pd.DataFrame(
            {
                "house_number": np.arange(1, 32_769),
                "STATE": ["56"] * 32_768,
                "PUMA": ["00100"] * 32_768,
            }
        )
        tracts_by_puma = pd.DataFrame(
            {"puma_id": ["5600100"], "tracts": [["56001000100"]]}
        )

        with pytest.raises(
            ValueError,
            match=(
                r"home sequence overflow for tract 56001000100: "
                r"maximum sequence 32768 exceeds FIPSCode limit 32767"
            ),
        ):
            assign_geography(synth_pop_df, tracts_by_puma, tracts_gdf, rng)


class TestBuildOutputs:
    def test_output_columns(self, geo_df):
        people_df, region_df = build_outputs(geo_df)
        assert list(people_df.columns) == [
            "age",
            "homeId",
            "schoolId",
            "workplaceId",
        ]
        assert list(region_df.columns) == ["censusTractId", "lat", "lon"]

    def test_region_deduped(self, geo_df):
        _, region_df = build_outputs(geo_df)
        assert region_df["censusTractId"].is_unique


class TestReproducibility:
    def test_same_seed_same_places(self, tracts_gdf):
        a = create_places(tracts_gdf, 5, "id", np.random.default_rng(1))
        b = create_places(tracts_gdf, 5, "id", np.random.default_rng(1))
        pd.testing.assert_frame_equal(a, b)

    def test_different_seed_different_places(self, tracts_gdf):
        a = create_places(tracts_gdf, 5, "id", np.random.default_rng(1))
        b = create_places(tracts_gdf, 5, "id", np.random.default_rng(2))
        assert not a["id"].equals(b["id"])
