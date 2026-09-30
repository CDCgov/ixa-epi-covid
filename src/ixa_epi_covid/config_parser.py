import json
import os
from pathlib import Path
from typing import Any

from calibrationtools import IndependentKernels, MultivariateNormalKernel, Particle, SeedKernel
import numpy as np
from particle_reader import ParticleReader
import yaml
from mrp.api import apply_dict_overrides

class ParticlesToParams:
    """Turn a particle into model parameters with a unique output subdir.

    A class rather than a closure so the sampler's callables stay pickleable and
    can cross the Azure Batch worker boundary.
    """

    def __init__(self, reader: ParticleReader):
        self.reader = reader

    def __call__(self, particle: Particle) -> dict[str, Any]:
        particle_params = self.reader.read_particle(particle=particle)
        # Make particle-specific output directory and update the output path in the parameters accordingly
        simulations_dir = Path(
            particle_params["config_inputs"]["output_dir"], "simulations"
        )
        # Count existing directories in simulations_dir
        if not simulations_dir.exists():
            dir_count = 0
        else:
            dir_count = len(os.walk(simulations_dir).__next__()[1])
        output_dir = Path(
            simulations_dir,
            ".".join(
                [
                    str(dir_count),
                    str(
                        particle_params["ixa_inputs"]["epimodel.GlobalParams"][
                            "seed"
                        ]
                    ),
                ]
            ),
        )
        output_dir.mkdir(parents=True, exist_ok=False)

        updated_params = update_epimodel_output_dir(
            particle_params, output_dir
        )
        return updated_params

class CovidModelConfig:
    """
    A class to handle the configuration for the COVID model calibration process. It reads a YAML config file, validates it, and provides methods to access the configuration parameters and generate default parameters for the MRP model based on the configuration.
    The initialization also handles the generation of the default ixa parameters and some convenience methods for

    Args:
        config_file (str | Path): The path to the YAML configuration file.
        ixa_overrides (dict, optional): A dictionary of overrides for the ixa parameters. Defaults to an empty dictionary.
        **kwargs: Additional keyword arguments that can be used to override values in the configuration file.
    """

    def __init__(
        self, config_file: str | Path, ixa_overrides: dict = {}, **kwargs
    ):
        with open(config_file, "r") as f:
            self.config = yaml.safe_load(f)

        self.config = apply_dict_overrides(self.config, kwargs)
        self._validate_config()

        self.config_file = config_file
        self.ixa_default_params_file = self.config["ixa_default_params_file"]

        with open(self.ixa_default_params_file, "r") as f:
            self.ixa_defaults = json.load(f)

        if ixa_overrides:
            self.update_ixa_params(ixa_overrides)

        self.priors: dict[dict, dict] = {}
        self.perturbation_kernel = None
        self.mrp_defaults: dict[str, Any] = {}
        self.particles_to_params: ParticlesToParams | None = None

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
    def set_reader(self):
        particle_param_names = list(self.priors["priors"].keys()) + ["seed"]
        print(f"Setting up ParticleReader with particle_param_names: {particle_param_names}")
        self.reader = ParticleReader(
            particle_param_names=particle_param_names,
            default_params=self.mrp_defaults,
        )
        self.particles_to_params = ParticlesToParams(self.reader)

    def set_mrp_defaults(self, output_dir: str | Path, outputs_to_read: list[str]):
        self.mrp_defaults = self.get_mrp_defaults_for_output(
            output_dir=output_dir, outputs_to_read=outputs_to_read
        )

    def set_priors(self, priors_file: str | Path):
        with open(priors_file, "r") as f:
            self.priors = json.load(f)
    
    def set_perturbation_kernel(self):
        self.perturbation_kernel = IndependentKernels(
    [
        MultivariateNormalKernel(list(self.priors["priors"].keys())),
        SeedKernel("seed"),
    ]
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


