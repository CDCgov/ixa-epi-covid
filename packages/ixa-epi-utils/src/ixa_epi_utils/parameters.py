import json
from pathlib import Path

import numpy as np


def load_baseline_parameters(
    file_path: str | Path, parameter_keyname: str
) -> dict:
    """Load baseline parameters from a JSON file."""
    with open(file_path, "r") as file:
        parameters = json.load(file)
    return parameters[parameter_keyname]


def sample_from_distributions(
    distributions: dict, n_samples: int = None, seed: int = None
) -> dict:
    """
    Sample parameters from given distributions.
    Eventually this should allow for correlated samples and other sampling techniques like sobol.
    For now, only independent sampling from uniform, normal, and beta distributions is supported.
    """

    if seed is not None:
        np.random.seed(seed)

    sampled_parameters = {}
    for param, dist_info in distributions.items():
        dist_type = dist_info["type"]
        match dist_type:
            case "uniform":
                sampled_parameters[param] = np.random.uniform(
                    low=dist_info["min"], high=dist_info["max"], size=n_samples
                )
            case "normal":
                sampled_parameters[param] = np.random.normal(
                    loc=dist_info["mean"],
                    scale=dist_info["std"],
                    size=n_samples,
                )
            case "beta":
                sampled_parameters[param] = np.random.beta(
                    a=dist_info["alpha"], b=dist_info["beta"], size=n_samples
                )
            case _:
                raise ValueError(f"Unknown distribution type: {dist_type}")
    return sampled_parameters


def update_params(base: dict, new: dict) -> dict:
    """Update base parameters with new parameters. Currently cannot handle nested dictionaries."""
    for value in base.values():
        if isinstance(value, dict):
            raise NotImplementedError(
                "Nested dictionary updates are not supported yet."
            )
    for value in new.values():
        if isinstance(value, dict):
            raise NotImplementedError(
                "Nested dictionary updates are not supported yet."
            )

    updated_params = base.copy()
    updated_params.update(new)
    return updated_params


def save_parameters(
    params: dict, file_path: str | Path, experiment_keyname: str
):
    """Save parameters to a JSON file."""
    with open(file_path, "w") as file:
        json.dump({experiment_keyname: params}, file, indent=4)
