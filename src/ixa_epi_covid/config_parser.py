import json
from pathlib import Path
from typing import Any

import yaml
from importation.geographies import get_state_proportion_population_data
from mrp.api import apply_dict_overrides


class CovidModelConfig:
    """Load and adapt phase-1 COVID model calibration configuration.

    Args:
        config_file (str | Path): Path to the YAML configuration file.
        ixa_overrides (dict[str, Any] | None): Optional IXA parameter
            overrides.
        **kwargs (Any): Additional configuration overrides.

    Raises:
        ValueError: If the config file is not a mapping or required keys are
            missing.
    """

    def __init__(
        self,
        config_file: str | Path,
        ixa_overrides: dict[str, Any] | None = None,
        **kwargs: Any,
    ):
        with open(config_file, "r") as f:
            loaded_config = yaml.safe_load(f)
        if not isinstance(loaded_config, dict):
            raise ValueError("Config file must contain a mapping.")

        self.config = apply_dict_overrides(loaded_config, kwargs)
        self._validate_config()

        self.config_file = config_file
        self.ixa_default_params_file = self.config["ixa_default_params_file"]

        with open(self.ixa_default_params_file, "r") as f:
            self.ixa_defaults = json.load(f)

        if ixa_overrides:
            self.update_ixa_params(ixa_overrides)

        # Set attributes from config for easy access
        for k in self.config.keys():
            self.__setattr__(k, self.config[k])

    @property
    def required_keys(self) -> set[str]:
        return {
            "exe_file",
            "force_overwrite",
            "state",
            "year",
            "symptomatic_reporting_prob",
            "ixa_default_params_file",
            "priors_file",
            "generation_particle_count",
            "tolerance_values",
            "target_data",
            "use_env_synth_pop_file",
        }

    def get_mrp_defaults_for_output(
        self, output_dir: str | Path, outputs_to_read: list[str]
    ) -> dict[str, Any]:
        """
        Generates the default parameters for the ixa-epi-covid MRP model based on the configuration and the specified output directory and outputs to read. It also updates the output directory in the epimodel parameters to point to the particle-specific output directory.

        Args:
            output_dir (str | Path): The path to the specified output directory.
            outputs_to_read (list[str]): A list of outputs to read to data from the model output products printed to disk.
        Returns:
            dict[str, Any]: The default parameters for the MRP model, with the output directory in the epimodel parameters updated to the user-specified output directory.
        """
        params = {
            "ixa_inputs": self.ixa_defaults,
            "config_inputs": {
                "exe_file": self.config["exe_file"],
                "force_overwrite": self.config["force_overwrite"],
                "outputs_to_read": outputs_to_read,
            },
            "importation_inputs": {
                "population_proportion": get_state_proportion_population_data(
                    state=self.config["state"],
                    year=self.config["year"],
                ),
                "state": self.config["state"],
                "year": self.config["year"],
                "symptomatic_reporting_prob": self.config[
                    "symptomatic_reporting_prob"
                ],
            },
        }
        return update_epimodel_output_dir(params, output_dir)

    def update_ixa_params(self, ixa_overrides: dict[str, Any]):
        """
        Updates the ixa parameters in the configuration with any provided overrides. This allows for dynamic updates to the ixa parameters without needing to modify the default parameters file directly.
        Args:
            ixa_overrides (dict[str, Any]): A dictionary of overrides for the ixa parameters, which do not require epimodel.GlobalParams but may include it as a header to be skipped.
        """
        if "epimodel.GlobalParams" in ixa_overrides:
            ixa_overrides = ixa_overrides["epimodel.GlobalParams"]
        self.ixa_defaults = apply_dict_overrides(
            self.ixa_defaults, {"epimodel.GlobalParams": ixa_overrides}
        )

    def _validate_config(self):
        """
        Validates that all required keys are present in the configuration.
        Raises:
            ValueError: if any required keys are missing.
        """
        missing_keys = self.required_keys - set(self.config.keys())
        if missing_keys:
            raise ValueError(
                f"Missing required keys in config: {missing_keys}"
            )


def update_epimodel_output_dir(
    particle_params: dict[str, Any], output_dir: str | Path
) -> dict[str, Any]:
    """
    Updates the output directory in the epimodel parameters to point to the particle-specific output directory.
    Args:
        particle_params (dict[str, Any]): The parameters for a specific particle, which includes the default parameters merged with the particle-specific parameters.
        output_dir (str | Path): The path to the particle-specific output directory.
    Returns:
        dict[str, Any]: The updated parameters with the output directory in the epimodel parameters updated to the particle-specific output directory.
    """
    params = apply_dict_overrides(
        particle_params,
        {
            "config_inputs": {"output_dir": str(output_dir)},
            "ixa_inputs": {
                "epimodel.GlobalParams": {
                    "imported_cases_timeseries": {
                        "filename": str(
                            Path(output_dir, "imported_cases_timeseries.csv")
                        )
                    }
                }
            },
        },
    )
    return params
