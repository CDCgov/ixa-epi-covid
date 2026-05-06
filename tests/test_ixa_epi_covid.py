import json
import tempfile
from pathlib import Path
from typing import Any

import polars as pl
import pytest
import yaml

from ixa_epi_covid import CovidModelConfig, update_epimodel_output_dir
from ixa_epi_covid.covid_model import IxaEpiCovidDirectRunner
from ixa_epi_covid.model_execution import (
    PHASE1_OUTPUT_NAME,
    generate_importation_timeseries,
)


@pytest.fixture
def valid_config():
    config = {
        "exe_file": "path/to/exe",
        "force_overwrite": True,
        "state": "California",
        "year": 2020,
        "symptomatic_reporting_prob": 0.5,
        "ixa_default_params_file": "./input/input.json",
        "priors_file": "./some_path/priors.json",
        "generation_particle_count": 100,
        "tolerance_values": [20.0, 15.0, 10.0],
        "target_data": {"t": [75], "count": [1]},
        "use_env_synth_pop_file": False,
    }
    return config


@pytest.fixture
def invalid_config():
    # Missing required keys
    config = {
        "exe_file": "path/to/exe",
        "force_overwrite": True,
        # Missing state, year, symptomatic_reporting_prob, etc.
    }
    return config


def test_update_epimodel_output_dir():
    # Given
    particle_params = {
        "ixa_inputs": {
            "epimodel.GlobalParams": {
                "imported_cases_timeseries": {"filename": "default/filename"}
            }
        },
        "config_inputs": {
            "exe_file": "path/to/exe",
            "force_overwrite": True,
            "outputs_to_read": ["output1", "output2"],
            "output_dir": "default/output/dir",
        },
        "importation_inputs": {
            "population_proportion": 0.12,
            "state": "California",
            "year": 2020,
            "symptomatic_reporting_prob": 0.5,
        },
    }
    output_dir = "particle/specific/output/dir"

    # When
    updated_params = update_epimodel_output_dir(particle_params, output_dir)

    # Then
    assert updated_params["config_inputs"]["output_dir"] == output_dir
    assert (
        updated_params["ixa_inputs"]["epimodel.GlobalParams"][
            "imported_cases_timeseries"
        ]["filename"]
        == "particle/specific/output/dir/imported_cases_timeseries.csv"
    )

    # Assert that other parameters are unchanged
    assert updated_params["importation_inputs"]["state"] == "California"
    assert (
        updated_params["importation_inputs"]["population_proportion"] == 0.12
    )
    assert updated_params["config_inputs"]["outputs_to_read"] == [
        "output1",
        "output2",
    ]


def test_direct_runner_honors_staged_input_output_and_run_id(
    monkeypatch,
    tmp_path,
):
    input_path = tmp_path / "input" / "gen_0_particle_0_attempt_0.json"
    output_dir = tmp_path / "output" / "generation-0"
    input_path.parent.mkdir(parents=True)
    input_path.write_text(
        json.dumps(
            {
                "config_inputs": {"output_dir": "original-output"},
                "ixa_inputs": {
                    "epimodel.GlobalParams": {
                        "imported_cases_timeseries": {
                            "filename": "original-output/imports.csv"
                        }
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    captured: dict[str, Any] = {}

    def fake_execute(model_inputs):
        captured.update(model_inputs)
        return {
            PHASE1_OUTPUT_NAME: pl.DataFrame(
                {
                    "t_lower": [0.0],
                    "t_upper": [1.0],
                    "count": [1],
                }
            )
        }

    monkeypatch.setattr(
        "ixa_epi_covid.covid_model.execute_phase1_model",
        fake_execute,
    )

    output = IxaEpiCovidDirectRunner().simulate(
        {"ignored": True},
        input_path=input_path,
        output_dir=output_dir,
        run_id="gen_0_particle_0_attempt_0",
    )

    assert captured["run_id"] == "gen_0_particle_0_attempt_0"
    assert captured["config_inputs"]["output_dir"] == str(output_dir)
    assert captured["ixa_inputs"]["epimodel.GlobalParams"][
        "imported_cases_timeseries"
    ]["filename"] == str(output_dir / "imported_cases_timeseries.csv")
    assert (output_dir / "output.csv").exists()
    assert output[PHASE1_OUTPUT_NAME] == {
        "t_lower": [0.0],
        "t_upper": [1.0],
        "count": [1],
    }
    json.dumps(output)


def test_get_mrp_defaults_for_output_includes_population_proportion(
    valid_config,
    monkeypatch,
    tmp_path,
):
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(valid_config, tmp)
        tmp_path_config = tmp.name

    monkeypatch.setattr(
        "ixa_epi_covid.config_parser.get_state_proportion_population_data",
        lambda state, year: 0.123,
    )

    config = CovidModelConfig(tmp_path_config)

    params = config.get_mrp_defaults_for_output(
        tmp_path / "output",
        outputs_to_read=["aggregated_deaths_report"],
    )

    assert params["importation_inputs"]["population_proportion"] == 0.123


def test_generate_importation_timeseries_uses_population_proportion(
    monkeypatch,
    tmp_path,
):
    captured_kwargs = {}

    class FakeTimeseries:
        def write_csv(self, path):
            Path(path).write_text(
                "time,imported_infections\n0,0\n",
                encoding="utf-8",
            )

    class FakeImportationModel:
        def __init__(self, *args, **kwargs):
            pass

        def sample_state_importation_incidence(self, **kwargs):
            captured_kwargs.update(kwargs)
            return FakeTimeseries()

    monkeypatch.setattr(
        "ixa_epi_covid.model_execution.ImportationModel",
        FakeImportationModel,
    )
    monkeypatch.setattr(
        "ixa_epi_covid.model_execution.get_linelist_data",
        lambda: object(),
    )

    model_inputs = {
        "config_inputs": {
            "output_dir": str(tmp_path / "output"),
        },
        "importation_inputs": {
            "population_proportion": 0.123,
            "state": "Indiana",
            "symptomatic_reporting_prob": 0.5,
            "year": 2020,
        },
        "ixa_inputs": {
            "epimodel.GlobalParams": {
                "imported_cases_timeseries": {
                    "filename": str(tmp_path / "imports.csv"),
                },
                "probability_critical_given_severe": {"A": 0.2},
                "probability_dead_given_critical": {"A": 0.1},
                "probability_mild_given_infect": 0.7,
                "probability_severe_given_mild": {"A": 0.3},
                "seed": 123,
                "symptom_age_groups": [{"label": "A", "min": 0, "max": 120}],
            }
        },
    }

    generate_importation_timeseries(model_inputs)

    assert captured_kwargs == {
        "proportion": 0.123,
        "state": "Indiana",
        "year": 2020,
    }


def test_covid_model_config_validation(valid_config):
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(valid_config, tmp)
        tmp_path = tmp.name

    # When / Then
    try:
        config = CovidModelConfig(tmp_path)
    except ValueError:
        pytest.fail(
            "CovidModelConfig raised ValueError unexpectedly with valid config!"
        )

    assert config.exe_file == valid_config["exe_file"]
    assert config.force_overwrite == valid_config["force_overwrite"]
    assert config.state == valid_config["state"]
    assert config.year == valid_config["year"]
    assert (
        config.symptomatic_reporting_prob
        == valid_config["symptomatic_reporting_prob"]
    )
    assert (
        config.ixa_default_params_file
        == valid_config["ixa_default_params_file"]
    )
    assert config.priors_file == valid_config["priors_file"]
    assert (
        config.generation_particle_count
        == valid_config["generation_particle_count"]
    )
    assert config.tolerance_values == valid_config["tolerance_values"]
    assert config.target_data == valid_config["target_data"]
    assert (
        config.use_env_synth_pop_file == valid_config["use_env_synth_pop_file"]
    )


def test_covid_model_config_validation_invalid(invalid_config):
    # Test missing keys
    invalid_config = {
        "exe_file": "path/to/exe",
        "force_overwrite": True,
        # Missing state, year, symptomatic_reporting_prob, etc.
    }
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(invalid_config, tmp)
        invalid_tmp_path = tmp.name
    with pytest.raises(ValueError):
        CovidModelConfig(invalid_tmp_path)


def test_covid_model_config_kwargs_updates(valid_config):
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(valid_config, tmp)
        tmp_path = tmp.name

    config = CovidModelConfig(tmp_path, state="Indiana")

    assert config.state == "Indiana"
    assert config.year == valid_config["year"]  # Unchanged


def test_covid_model_config_kwargs_supplements_invalid(invalid_config):
    # Test missing keys
    invalid_config = {
        "exe_file": "path/to/exe",
        "force_overwrite": True,
        # Missing state, year, symptomatic_reporting_prob, etc.
    }
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(invalid_config, tmp)
        invalid_tmp_path = tmp.name

    try:
        config = CovidModelConfig(
            invalid_tmp_path,
            state="Indiana",
            year=2020,
            symptomatic_reporting_prob=0.5,
            ixa_default_params_file="./input/input.json",
            priors_file="./some_path/priors.json",
            generation_particle_count=100,
            tolerance_values=[20.0, 15.0, 10.0],
            target_data={"t": [75], "count": [1]},
            use_env_synth_pop_file=False,
        )
    except ValueError:
        pytest.fail(
            "CovidModelConfig raised ValueError unexpectedly when kwargs supplemented missing required keys!"
        )

    assert config.state == "Indiana"
    assert config.year == 2020
    assert config.symptomatic_reporting_prob == 0.5
    assert config.ixa_default_params_file == "./input/input.json"
    assert config.priors_file == "./some_path/priors.json"
    assert config.generation_particle_count == 100
    assert config.tolerance_values == [20.0, 15.0, 10.0]
    assert config.target_data == {"t": [75], "count": [1]}
    assert not config.use_env_synth_pop_file


def test_covid_model_config_kwargs_supplements_invalid_missing_keys(
    invalid_config,
):
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(invalid_config, tmp)
        invalid_tmp_path = tmp.name

    with pytest.raises(ValueError):
        CovidModelConfig(
            invalid_tmp_path,
            year=2020,
            symptomatic_reporting_prob=0.5,
            ixa_default_params_file="./input/input.json",
            priors_file="./some_path/priors.json",
            generation_particle_count=100,
            tolerance_values=[20.0, 15.0, 10.0],
            target_data={"t": [75], "count": [1]},
            use_env_synth_pop_file=False,
        )


def test_update_ixa_params(valid_config):
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as tmp:
        yaml.dump(valid_config, tmp)
        tmp_path = tmp.name

    config = CovidModelConfig(tmp_path)
    ixa_defaults = config.ixa_defaults.copy()
    ixa_overrides = {
        "imported_cases_timeseries": {"filename": "overridden/filename"}
    }
    config.update_ixa_params(ixa_overrides)

    assert (
        config.ixa_defaults["epimodel.GlobalParams"][
            "imported_cases_timeseries"
        ]["filename"]
        == "overridden/filename"
    )
    assert (
        ixa_defaults["epimodel.GlobalParams"]["prevalence_report"]["filename"]
        == config.ixa_defaults["epimodel.GlobalParams"]["prevalence_report"][
            "filename"
        ]
    )
