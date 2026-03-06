from typing import Literal

import polars as pl

from .geographies import get_state_proportion_population_data
from .perkins_et_al_methods import validate_data as validate_multinomial_data


def summarize_linelist_importation_data(
    data: pl.DataFrame, report_day_bounds: tuple[int, int], expand: bool = True
) -> pl.DataFrame:
    """
    Summarize linelist importation data by report day and replicate.
    This function takes a DataFrame containing linelist importation data and
    generates a summary of the number of infections for each combination of report
    day. The resulting DataFrame contains two columns: `time`, and `imported_infections`.
    Args:
        data (pl.DataFrame): A Polars DataFrame containing linelist importation data
        report_day_bounds (tuple[int, int]): A tuple specifying the lower and upper bounds of report days to include in the summary.
        expand (bool, optional): Whether to expand the summary to include all report days within the specified bounds, filling missing days with zero infections. Defaults to True.
    Returns:
        pl.DataFrame: A Polars DataFrame containing the summarized importation incidence data, with columns `time` (report day) and `imported_infections` (number of infections reported on that day).
    """

    reporting_incidence = data.group_by("report_day").agg(
        pl.count().alias("imported_infections")
    )

    if expand:
        # Fill null values with zero incidence for unreported days
        all_days = pl.DataFrame(
            {
                "report_day": pl.int_range(
                    start=report_day_bounds[0],
                    end=report_day_bounds[1],
                    step=1,
                    eager=True,
                )
            }
        )

        reporting_incidence = (
            all_days.join(reporting_incidence, on="report_day", how="left")
            .fill_null(0)
            .sort("report_day")
            .select(["report_day", "imported_infections"])
            .rename({"report_day": "time"})
        )
    else:
        reporting_incidence = reporting_incidence.rename(
            {"report_day": "time"}
        ).select(["time", "imported_infections"])

    return reporting_incidence.with_columns(pl.col("time").cast(pl.Float64))


class RegionalModel:
    """
    Class for sampling importation data at the regional level (e.g., national or state level)
    Args:
        model_type (Literal["multinomial", "proportional"]): The type of regional model to use.
        parameters (dict | pl.DataFrame): A single set of parameters to be used for the model specification.
    Methods:
        sample_importation_data(reporting_data, seed=None, **kwargs) -> pl.DataFrame:
            Sample importation data based on the specified model type and parameters.
    """

    def __init__(
        self,
        model_type: Literal["multinomial", "proportional"],
        parameters: dict | pl.DataFrame,
    ):
        self.model_type = model_type
        self.parameters = parameters

    def sample_importation_data(
        self, reporting_data: pl.DataFrame, seed: int | None = None, **kwargs
    ) -> pl.DataFrame:
        """
        Sample importation data based on the specified model type and parameters.
        Args:
            reporting_data (pl.DataFrame): A Polars DataFrame containing the reporting data to sample from.
            seed (int | None): A random seed for reproducibility. Defaults to None.
            **kwargs: Additional keyword arguments that may be required for specific model types.
        Returns:
            pl.DataFrame: A Polars DataFrame containing the sampled importation data.
        Raises:
            ValueError: If an unknown model type is specified or if required parameters are missing for the proportional model.
        Notes:
            - For the "multinomial" model type, the sampling is performed using the `sample_us_importation_incidence_data` function from the `perkins_et_al_methods` module, which requires specific parameters to be provided in `self.parameters`.
        """
        match self.model_type:
            case "multinomial":
                from .perkins_et_al_methods import (
                    sample_us_importation_incidence_data,
                )

                return sample_us_importation_incidence_data(
                    reporting_data=reporting_data,
                    importation_parameters=self.parameters,
                    max_infections=self.parameters.get(
                        "max_infections", 20000
                    ),
                    seed=seed,
                )
            case "proportional":
                if "proportion" in kwargs:
                    self.parameters.update(
                        {"proportion": kwargs["proportion"]}
                    )
                elif "proportion" not in self.parameters:
                    if "state" in kwargs:
                        yr = kwargs.get("year")
                        state_proportion_data = (
                            get_state_proportion_population_data(
                                state=kwargs["state"], year=yr
                            )
                        )
                        self.parameters.update(
                            {"proportion": state_proportion_data}
                        )
                    else:
                        raise ValueError(
                            "State must be provided for proportional model if not specified explicitly in inputs"
                        )

                return reporting_data.sample(
                    fraction=self.parameters["proportion"],
                    with_replacement=kwargs.get("with_replacement", False),
                    seed=seed,
                )
            case _:
                raise ValueError(
                    f"Unknown regional model type: {self.model_type}"
                )


class ImportationModel:
    """
    A model for simulating and processing synthetic importation data at both national and state levels.

    Args:
        data (pl.DataFrame): The input data for the model.
        parameters (dict | pl.DataFrame): A single set of parameters to be used for the model specification.
        state_model (Literal["multinomial", "proportional"]): The type of the state model.
        national_model (Literal["multinomial"] | None): The type of the national model, if specified.
        seed (int | None): The random seed for reproducibility.

    Methods:
        _validate_model_inputs() -> None:
            Validates the model inputs to ensure that the specified national and state models are compatible and that the input data is suitable for the multinomial model if either model is specified as multinomial.
        sample_state_importation_incidence(fill_null=True, seed=None, **kwargs) -> pl.DataFrame:
            Samples state-level importation incidence data. If a national model is specified,
            it first generates importation data at the national level and then samples state-level
            data from it. Otherwise, it directly samples state-level data from the input data.
        summarize_linelist_importation_data(linelist_data, expand=True) -> pl.DataFrame:
            Summarizes the linelist importation data over a specified range of report days in self.data.
            Optionally expands the data to fill missing day values with zero.
    Notes:
    - The model supports only one type of national model (multinomial) and requires that if a national model is specified, the state model cannot be multinomial. If no national model is specified, the state model must be multinomial.
    - The input data must be validated for compatibility with the multinomial model if either the national or state model is specified as multinomial.
    """

    def __init__(
        self,
        data: pl.DataFrame,
        parameters: dict | pl.DataFrame,
        state_model: Literal["multinomial", "proportional"],
        national_model: Literal["multinomial"] | None = None,
        seed: int | None = None,
    ):
        self.parameters = parameters
        self.national_model_type = national_model
        self.state_model_type = state_model
        self.data = data
        self.seed = seed
        self._validate_model_inputs()

        if self.national_model_type:
            self.national_model = RegionalModel(
                self.national_model_type, parameters
            )
        else:
            self.national_model = None
        self.state_model = RegionalModel(self.state_model_type, parameters)

    def _validate_model_inputs(self):
        """
        Validates the model inputs to ensure that the specified national and state models are compatible and that the input data is suitable for the multinomial model if either model is specified as multinomial.
        """
        if self.national_model_type:
            assert self.national_model_type == "multinomial", (
                "Unknown national model type specified"
            )
            assert self.state_model_type != "multinomial", (
                "State model cannot be multinomial when national model is specified"
            )
        else:
            assert self.state_model_type == "multinomial", (
                "State model must be multinomial if national model is not specified"
            )

        assert self.state_model_type in ["multinomial", "proportional"], (
            "Unknown state model type specified"
        )

        if (
            self.national_model_type == "multinomial"
            or self.state_model_type == "multinomial"
        ):
            validate_multinomial_data(self.data)

    def sample_state_importation_incidence(
        self, fill_null: bool = True, seed: int | None = None, **kwargs
    ) -> pl.DataFrame:
        """
        Samples state-level importation incidence data. If a national model is specified, it first generates importation data at the national level and then samples state-level data from it.
        Args:
            fill_null (bool, optional): Whether to fill null values with zero incidence for unreported days in the final summarized data. Defaults to True.
            seed (int | None): A random seed for reproducibility. If not provided, the seed specified during model initialization will be used. Defaults to None.
            **kwargs: Additional keyword arguments that may be required for specific model types when sampling importation data.
        Returns:
            pl.DataFrame: A Polars DataFrame containing the summarized state-level importation incidence data, with columns `time` (report day) and `imported_infections` (number of infections imported on that day).
        """
        if not seed:
            seed = self.seed
            # If a national model is specified, generate importation data at the national level first, then sample state-level importation data from the national-level data.
            # If no national model is specified, sample state-level importation data directly from the input data.
        if self.national_model:
            importation_data = self.national_model.sample_importation_data(
                self.data, seed=seed
            )
        else:
            importation_data = self.data

        state_data = self.state_model.sample_importation_data(
            importation_data, seed=seed, **kwargs
        )
        return self.summarize_linelist_importation_data(
            state_data, expand=fill_null
        )

    def summarize_linelist_importation_data(
        self, linelist_data: pl.DataFrame, expand: bool = True
    ) -> pl.DataFrame:
        """
        Summarizes the linelist importation data over a specified range of report days in self.data. Optionally expands the data to fill missing day values with zero.
        Args:
            linelist_data (pl.DataFrame): A Polars DataFrame containing the linelist importation data to be summarized. It must include a column named "report_day" that indicates the day of reporting for each detected infection.
            expand (bool, optional): Whether to expand the summarized data to include all report days within the range of report days in self.data, filling missing days with zero infections. Defaults to True.
        Returns:
            pl.DataFrame: A Polars DataFrame containing the summarized importation incidence data, with columns `time` (report day) and `imported_infections` (number of infections imported).
        """
        low = min(self.data.select(pl.col("report_day").min()).item(), 0)
        high = self.data.select(pl.col("report_day").max()).item() + 1
        return summarize_linelist_importation_data(
            linelist_data, (low, high), expand=expand
        )
