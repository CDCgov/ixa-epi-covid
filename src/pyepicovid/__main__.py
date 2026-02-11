import os

import importation
import ixa_epi_utils as utils
import polars as pl
from epimodel import EpiModel
from importation import ImportationModel


def main():
    ## Load the baseline parameters for the epi model, which include the seed for sampling importation parameters
    baseline_params = utils.load_baseline_parameters(
        file_path="./input/input.json",
        parameter_keyname="epimodel.GlobalParams",
    )

    ## Generate an importation curve for the state of Indiana
    importation_priors = importation.get_importation_parameter_dict()
    importation_params = utils.sample_from_distributions(
        distributions=importation_priors,
        seed=baseline_params["seed"],
    )

    importation_model = ImportationModel(
        data=importation.get_data(),
        parameters=importation_params,
        national_model="multinomial",
        state_model="proportional",
        seed=baseline_params["seed"],
    )

    state_proportion = 0.023

    imported_infections_indiana = (
        importation_model.sample_state_importation_incidence(
            proportion=state_proportion
        )
    )

    ## Save the generated importation data and parameters for use in the epi model
    importation_filename = "./input/indiana_importation.csv"
    imported_infections_indiana.write_csv(importation_filename)

    params = utils.update_params(
        base=baseline_params,
        new={"importation_filename": importation_filename},
    )
    params = utils.update_params(base=params, new=importation_params)

    input_filename = "./input/epimodel_parameters.json"

    utils.save_parameters(
        params,
        file_path=input_filename,
        experiment_keyname="epimodel.GlobalParams",
    )

    ## Run the model
    epi_model = EpiModel(config_file=input_filename, force_overwrite=True)
    epi_model.run()

    ## Clean up generated input file and overwrite importation data with summary
    os.remove(input_filename)
    (
        imported_infections_indiana.filter(pl.col("imported_infections") > 0)
        .join(pl.DataFrame(importation_params), how="cross")
        .write_csv("./output/importation_summary.csv")
    )


if __name__ == "__main__":
    main()
