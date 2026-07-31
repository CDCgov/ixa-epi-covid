from pathlib import Path

import polars as pl

relationship_file = Path("input/school_district_tract_mapping.csv")

def impacted_tracts(
    district: str,
    separator: str = ",",
) -> pl.DataFrame:
    """
    Return census tracts intersecting an NCES school district.

    Parameters
    ----------
    district:
        Seven-digit NCES LEAID or an exact district name.
        LEAID is preferred because district names may not be unique.

    separator:
        File delimiter. Use "," for CSV, "\\t" for tab-delimited,
        or "|" for pipe-delimited files.
    """
    relationships = pl.read_csv(
        relationship_file,
        separator=separator,
        infer_schema=False,  # Preserve identifiers as strings.
    )

    relationships = relationships.rename(
        {column: column.upper() for column in relationships.columns}
    )

    required_columns = {
        "LEAID",
        "TRACT",
    }

    missing_columns = required_columns - set(relationships.columns)

    if missing_columns:
        raise ValueError(
            f"Relationship file is missing columns: "
            f"{sorted(missing_columns)}"
        )

    # The name field contains the release year, such as NAME_LEA24.
    name_columns = [
        column
        for column in relationships.columns
        if column.startswith("NAME_LEA")
    ]

    if len(name_columns) != 1:
        raise ValueError(
            "Could not identify a unique NAME_LEAxx column. "
            f"Found: {name_columns}"
        )

    name_column = name_columns[0]
    district_argument = str(district).strip()

    # A numeric argument is interpreted as an NCES LEAID.
    if district_argument.isdigit():
        district_id = district_argument.zfill(7)

        matches = relationships.filter(
            pl.col("LEAID") == district_id
        )
    else:
        matches = relationships.filter(
            pl.col(name_column)
            .str.strip_chars()
            .str.to_lowercase()
            == district_argument.lower()
        )

        candidates = matches.select(
            "LEAID",
            name_column,
        ).unique()

        if candidates.height > 1:
            raise ValueError(
                "District name is not unique. Use one of these LEAIDs: "
                f"{candidates.to_dicts()}"
            )

    if matches.height == 0:
        raise LookupError(
            f"No school district found for {district!r}"
        )

    result = (
        matches
        .select(
            pl.col("LEAID").alias("district_id"),
            pl.col(name_column).alias("district_name"),
            pl.col("TRACT").alias("tract_geoid"),
        )
    )

    return result

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import polars as pl


def plot_districts_per_tract(
    lea_data: pl.DataFrame,
    state_fips: str | int,
    output_path: str | Path | None = None,
    show: bool = True,
) -> tuple[pl.DataFrame, plt.Figure, plt.Axes]:
    """
    Plot the number of unique school districts intersecting each tract
    within a specified state.

    Parameters
    ----------
    lea_data:
        Polars DataFrame containing LEAID and TRACT columns.

    state_fips:
        State FIPS code, such as "06" or 6 for California.

    output_path:
        Optional destination for the histogram image.

    show:
        Whether to display the plot.
    """
    state_fips = str(state_fips).strip().zfill(2)

    if len(state_fips) != 2 or not state_fips.isdigit():
        raise ValueError(
            "state_fips must be a one- or two-digit state FIPS code."
        )

    lea_data = lea_data.rename(
        {column: column.upper() for column in lea_data.columns}
    )

    required_columns = {"LEAID", "TRACT"}
    missing_columns = required_columns - set(lea_data.columns)

    if missing_columns:
        raise ValueError(
            f"LEA data is missing columns: {sorted(missing_columns)}"
        )

    tract_counts = (
        lea_data
        .filter(
            pl.col("LEAID").is_not_null()
            & pl.col("TRACT").is_not_null()
            & pl.col("TRACT").str.starts_with(state_fips)
        )
        .unique(subset=["TRACT", "LEAID"])
        .group_by("TRACT")
        .agg(
            pl.col("LEAID")
            .n_unique()
            .alias("school_district_count")
        )
        .sort("TRACT")
    )

    if tract_counts.height == 0:
        raise ValueError(
            f"No tract-district relationships found for "
            f"state FIPS {state_fips}."
        )

    district_counts = (
        tract_counts
        .get_column("school_district_count")
        .to_numpy()
    )

    maximum_count = int(district_counts.max())
    bins = np.arange(0.5, maximum_count + 1.5, 1)

    figure, axes = plt.subplots(figsize=(10, 6))

    axes.hist(
        district_counts,
        bins=bins,
        color="#386CB0",
        edgecolor="white",
        linewidth=1,
    )

    axes.set(
        title=(
            "School Districts Intersecting Each Census Tract\n"
            f"State FIPS: {state_fips}"
        ),
        xlabel="Number of school districts",
        ylabel="Number of census tracts",
    )

    axes.set_xticks(range(1, maximum_count + 1))
    axes.grid(axis="y", alpha=0.25)
    figure.tight_layout()

    if output_path is not None:
        output_path = Path(output_path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        figure.savefig(output_path, dpi=300, bbox_inches="tight")

    if show:
        plt.show()

    return tract_counts, figure, axes

def plot_districts_per_county(
    lea_county_data: pl.DataFrame,
    state_fips: str | int,
    output_path: str | Path | None = None,
    show: bool = True,
) -> tuple[pl.DataFrame, plt.Figure, plt.Axes]:
    """
    Plot the number of unique school districts intersecting each county
    within a specified state.

    Parameters
    ----------
    lea_county_data:
        Polars DataFrame from the NCES grfXX_lea_county file. It must
        contain LEAID and STCOUNTY columns.

    state_fips:
        State FIPS code, such as "06" or 6 for California.

    output_path:
        Optional destination for the histogram image.

    show:
        Whether to display the histogram.

    Returns
    -------
    county_counts, figure, axes
        county_counts contains one row per county and the number of
        unique school districts intersecting it.
    """
    state_fips = str(state_fips).strip().zfill(2)

    if len(state_fips) != 2 or not state_fips.isdigit():
        raise ValueError(
            "state_fips must be a one- or two-digit state FIPS code."
        )

    lea_county_data = lea_county_data.rename(
        {
            column: column.upper()
            for column in lea_county_data.columns
        }
    )

    required_columns = {"LEAID", "STCOUNTY"}
    missing_columns = required_columns - set(lea_county_data.columns)

    if missing_columns:
        raise ValueError(
            f"LEA county data is missing columns: "
            f"{sorted(missing_columns)}"
        )

    # NCES names this field according to the release year,
    # such as NAME_COUNTY24 or NAME_COUNTY25.
    county_name_columns = [
        column
        for column in lea_county_data.columns
        if column.startswith("NAME_COUNTY")
    ]

    county_counts = lea_county_data.filter(
        pl.col("LEAID").is_not_null()
        & pl.col("STCOUNTY").is_not_null()
        & pl.col("STCOUNTY").str.starts_with(state_fips)
    ).unique(subset=["STCOUNTY", "LEAID"]).group_by("LEAID").agg(
        pl.col("STCOUNTY")
        .n_unique()
        .alias("county_count")
    )
    
    maximum_count = int(county_counts.get_column("county_count").max())
    bins = np.arange(0.5, maximum_count + 1.5, 1)

    figure, axes = plt.subplots(figsize=(10, 6))

    axes.hist(
        county_counts.get_column("county_count").to_numpy(),
        bins=bins,
        color="#386CB0",
        edgecolor="white",
        linewidth=1,
    )

    axes.set(
        title=(
            "Number of Counties in a school district\n"
            f"State FIPS: {state_fips}"
        ),
        xlabel="Number of counties",
        ylabel="Number of school districts",
    )

    axes.set_xticks(range(1, maximum_count + 1))
    axes.grid(axis="y", alpha=0.25)
    figure.tight_layout()

    if output_path is not None:
        output_path = Path(output_path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        figure.savefig(output_path, dpi=300, bbox_inches="tight")

    if show:
        plt.show()

    return county_counts, figure, axes

if __name__ == "__main__":
    county_file = Path("input/grf25_lea_county.csv")
    lea_data = pl.read_csv(
        county_file,
        infer_schema=False,
    )   

    county_counts, figure, axes = plot_districts_per_county(
        lea_county_data=lea_data,
        state_fips="18",
        output_path="indiana_districts_per_county.png",
    )