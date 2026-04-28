import inspect
from typing import Any, Callable, Concatenate

from calibrationtools.particle import Particle
from mrp.api import apply_dict_overrides


def flatten_dict(
    structured_dict: dict[str, Any],
    parent_key: str = "",
    escape_sep: bool = True,
) -> dict[str, Any]:
    """
    Flattens a nested dictionary by concatenating keys with a '.' separator.

    Args:
        structured_dict (dict[str, Any]): The nested dictionary to flatten.
        parent_key (str): The base key to prepend to each key in the flattened dictionary. Defaults to an empty string.
        escape_sep (bool): Whether to escape the separator in keys that contain it. If True, occurrences of the separator in keys will be prefixed with a backslash. If False, a ValueError will be raised if any key contains the separator. Defaults to True.

    Returns:
        dict[str, Any]: A flattened dictionary where nested keys are concatenated with the specified separator.
    Raises:
        ValueError: If escape_sep is False and any key in the structured_dict contains the separator
    """
    items = []
    for k, v in structured_dict.items():
        if "." in k:
            if escape_sep:
                k = k.replace(".", "\\.")
            else:
                raise ValueError(
                    f"Key '{k}' contains the separator '.' and escape_sep is set to False."
                )

        new_key = ".".join([parent_key, k]) if parent_key else k
        if isinstance(v, dict):
            items.extend(flatten_dict(v, new_key).items())
        else:
            items.append((new_key, v))
    return dict(items)


def unflatten_parameter_name(
    flattened_name: str, value: Any
) -> dict[str, Any]:
    """
    Unflattens a parameter name by splitting it on dots.

    Args:
        flattened_name (str): The flattened parameter name (e.g., "offspring_distribution.NegativeBinomial.mean").
        value (Any): The value to assign to the unflattened parameter.
    Returns:
        dict[str, Any]: A dictionary representing the unflattened parameter name (e.g., {"offspring_distribution": {"NegativeBinomial": {"mean2": value}}}).
    """
    name_vec = flattened_name.split(".")
    param_vec = []
    i = 0
    # Corrct for any escaped separators introduced during flattening
    while i < len(name_vec):
        if name_vec[i].endswith("\\"):
            param_vec.append(name_vec[i].replace("\\", ".") + name_vec[i + 1])
            i += 2
        else:
            param_vec.append(name_vec[i])
            i += 1

    param_dict = {param_vec[-1]: value}
    for key in reversed(param_vec[:-1]):
        param_dict = {key: param_dict}
    return param_dict


class ParticleReader:
    """
    ParticleReader is a utility class for converting Particle objects into dictionaries
    suitable for use as model parameter inputs. It supports merging particle-provided
    values into a nested default parameter structure, and also delegating conversion
    to a user-supplied reader function.
    Args:
        particle_param_names (list[str]): Flat parameter names expected to be present
            on Particle instances (e.g. ["transmission.rate", "recovery.mean"]).
        default_params (dict[str, Any] | None): Nested default parameter dictionary
            (possibly multi-level) that particle values should override. If provided,
            ParticleReader attempts to map each flat particle parameter name to a
            corresponding flattened key in this defaults dictionary. Defaults to None.
    Behavior:
        - When default_params is provided, the constructor flattens it (via
          flatten_dict) and attempts to match each particle_param_names entry to one
          flattened key by using string suffix matching (flat_name.endswith(param_name)).
          If no match is found for a parameter, a ValueError is raised. If multiple
          flattened keys match the same parameter name, a ValueError is raised to
          avoid ambiguity.
        - When no default_params are provided, particle parameters are treated as
          already representing the final (flat) names.
    """

    def __init__(
        self,
        particle_param_names: list[str],
        default_params: dict[str, Any] | None = None,
    ):
        self.default_params = default_params or {}
        self.particle_param_names = particle_param_names
        self.name_key = self._map_particle_params_to_defaults()

    def _map_particle_params_to_defaults(self) -> dict[str, str]:
        """
        Creates a mapping from flat particle parameter names to their corresponding flattened keys in the default_params structure.
        This mapping is used to guide the unflattening process when merging particle parameters with defaults.

        Raises:
            ValueError: If a particle parameter name cannot be uniquely matched to a flattened key in default_params (no matches or multiple matches).
        Returns:
            dict[str, str]: A mapping from flat particle parameter names to their corresponding flattened keys in the default_params structure.
        """
        if self.default_params:
            flat_names = flatten_dict(self.default_params)
            name_key = {}
            for param_name in self.particle_param_names:
                found_match_count = 0

                for flat_name in flat_names.keys():
                    if flat_name == param_name:
                        name_key.update({param_name: flat_name})
                        found_match_count = 1
                        break
                    elif flat_name.endswith("." + param_name):
                        name_key.update({param_name: flat_name})
                        found_match_count += 1
                if found_match_count == 0:
                    raise ValueError(
                        f"No matching default parameter found for '{param_name}'"
                    )
                elif found_match_count > 1:
                    raise ValueError(
                        f"Multiple matching default parameters found for '{param_name}'"
                    )
        else:
            name_key = {
                param_name: param_name
                for param_name in self.particle_param_names
            }
        return name_key

    def _merge_particle_with_defaults(
        self, particle: Particle
    ) -> dict[str, Any]:
        """
        Merges the parameters from a Particle with the default parameters, using the mapping defined in self.name_key to unflatten particle parameter names into the nested structure of default_params.

        Args:
            particle (Particle): The particle whose parameters are being merged with defaults.
        Returns:
            dict[str, Any]: A mapping from flat particle parameter names to their corresponding
            flattened keys in the default_params structure, used to guide the unflattening process.
        """
        particle_params = {}
        for param_name, value in particle.items():
            unflattened = unflatten_parameter_name(
                self.name_key[param_name], value
            )
            particle_params = apply_dict_overrides(
                particle_params, unflattened
            )
        merged_params = apply_dict_overrides(
            self.default_params, particle_params
        )
        return merged_params

    def read_particle(
        self,
        particle: Particle,
        read_fn: Callable[Concatenate[Particle, ...], dict] | None = None,
        **kwargs,
    ) -> dict[str, Any]:
        """
        Read a Particle into a dictionary using either a user-provided reader or the internal default merger.

        If a callable read_fn is provided, it is invoked to produce the particle dictionary. The method inspects
        read_fn's signature and will pass this instance's default_params as the keyword default_params if and only if
        the callable either accepts arbitrary keyword arguments (**kwargs) or declares a parameter named "default_params".
        All additional keyword arguments passed to this method are forwarded to read_fn.

        If read_fn is None, the particle is processed by the instance method _merge_particle_with_defaults and that
        result is returned.

        Args:
            particle (Particle): The particle object to read/convert into a dict.
            read_fn (Callable[Concatenate[Particle, ...], dict] | None): Optional user-supplied function to read the particle. It should
                accept the particle as its first argument and return a dict of attributes. If it accepts a parameter
                named "default_params" or accepts arbitrary keyword arguments, default_params from this instance will be
                supplied automatically.
            **kwargs: Additional keyword arguments forwarded to read_fn when provided.

        Returns:
            dict[str, Any]: A dictionary representation of the particle produced either by read_fn or by
            _merge_particle_with_defaults.
        """
        if read_fn is not None:
            # Be sure to pass default params to user-defined read_fn if expected
            args = inspect.signature(read_fn).parameters
            accepts_default = (
                any(
                    req.kind == inspect.Parameter.VAR_KEYWORD
                    for req in args.values()
                )
                or "default_params" in args
            )
            if accepts_default:
                return read_fn(
                    particle, default_params=self.default_params, **kwargs
                )
            else:
                return read_fn(particle, **kwargs)
        else:
            return self._merge_particle_with_defaults(particle)
