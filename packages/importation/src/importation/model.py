from typing import Literal

import polars as pl


def summarize_linelist_importation_data(
    data: pl.DataFrame, report_day_bounds: tuple[int, int], expand: bool = True
) -> pl.DataFrame:
    """
    Summarize linelist importation data by report day and replicate.
    This function takes a DataFrame containing linelist importation data and
    generates a summary of the number of infections for each combination of report
    day. The resulting DataFrame contains three columns: `day`, and `imported_infections`.
    Args:
        data (pl.DataFrame): A Polars DataFrame containing linelist importation data
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
            .rename({"report_day": "day"})
        )
    else:
        reporting_incidence = reporting_incidence.rename(
            {"report_day": "day"}
        ).select(["day", "imported_infections"])

    return reporting_incidence


class RegionalModel:
    def __init__(
        self,
        model_type: Literal["multinomial"],
        parameters: dict | pl.DataFrame,
    ):
        """Class for sampling importation data at the regional level (e.g., national or state level)"""
        self.model_type = model_type
        self.parameters = parameters

    def sample_importation_data(
        self, reporting_data, seed: int = None, **kwargs
    ) -> pl.DataFrame:
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
                    self.parameters.update({"proportion": kwargs["proportion"]})
                elif "proportion" not in self.parameters:
                    raise ValueError(
                        "Proportion parameter must be specified for proportional model"
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

    Attributes:
        parameters (dict | pl.DataFrame): The parameters for the model, which can be provided as a dictionary or a DataFrame.
        national_model_type (Literal["multinomial"]): The type of the national model. Defaults to None.
        national_model (RegionalModel | None): The national-level model instance, if specified.
        state_model_type (Literal["multinomial", "proportional"]): The type of the state model.
        state_model (RegionalModel): The state-level model instance.
        seed (int | None): The random seed for reproducibility.
        data (pl.DataFrame): The input data for the model.

    Methods:
        __init__(data, parameters, state_model, national_model=None, seed=None):
            Initializes the ImportationModel with the given data, parameters, and model types.

        sample_state_importation_incidence(fill_null=True, seed=None, **kwargs) -> pl.DataFrame:
            Samples state-level importation incidence data. If a national model is specified,
            it first generates importation data at the national level and then samples state-level
            data from it. Otherwise, it directly samples state-level data from the input data.

        summarize_linelist_importation_data(linelist_data, expand=True) -> pl.DataFrame:
            Summarizes the linelist importation data over a specified range of report days in self.data.
            Optionally expands the data to fill missing day values with zero.

    Raises:
        ValueError: If an unknown national model type is specified.
        AssertionError: If the state model is not "multinomial" and no national model is specified.
    """

    def __init__(
        self,
        data: pl.DataFrame,
        parameters: dict | pl.DataFrame,
        state_model: Literal["multinomial", "proportional"],
        national_model: Literal["multinomial"] = None,
        seed: int = None,
    ):
        self.parameters = parameters
        self.national_model_type = national_model
        if self.national_model_type:
            self.national_model = RegionalModel(
                self.national_model_type, parameters
            )
        else:
            assert state_model == "multinomial", (
                "State model must be multinomial if national model is not specified"
            )
            self.national_model = None
        self.state_model_type = state_model

        self.state_model = RegionalModel(self.state_model_type, parameters)
        self.seed = seed

        match national_model:
            case "multinomial":
                from .perkins_et_al_methods import validate_data

                validate_data(data)
                self.data = data
            case _:
                raise ValueError(
                    f"Unknown national model type: {self.national_model_type}"
                )

    def sample_state_importation_incidence(
        self, fill_null: bool = True, seed: int = None, **kwargs
    ) -> pl.DataFrame:
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
        low = min(self.data.select(pl.col("report_day").min()).item(), 0)
        high = self.data.select(pl.col("report_day").max()).item() + 1
        return summarize_linelist_importation_data(
            linelist_data, (low, high), expand=expand
        )
