import argparse
import csv
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable
import warnings

import polars as pl

relationship_file = Path("input/school_district_tract_mapping.csv")

OUTPUT_COLUMNS = [
    "geography",
    "state",
    "county",
    "school_district",
    "start_time",
    "end_time",
]

CLOSURE_SCHEMA = {
    "geography": pl.String,
    "state": pl.String,
    "county": pl.String,
    "school_district": pl.String,
    "start_time": pl.Float64,
    "end_time": pl.Float64,
}


def validate_columns(
    df: pl.DataFrame,
    required: set[str],
    *,
    source: str,
) -> None:
    missing = required.difference(df.columns)

    if missing:
        raise ValueError(
            f"{source} is missing required columns: "
            f"{', '.join(sorted(missing))}"
        )


def preprocess_school_closures(
    closures_path: Path,
    mapping_path: Path,
) -> pl.DataFrame:
    """Expand county closures to the school districts they contain."""

    # Read identifiers as strings so leading zeros are preserved.
    closures = pl.read_csv(
        closures_path,
        schema_overrides={
            "geography": pl.String,
            "state": pl.String,
            "county": pl.String,
            "school_district": pl.String,
        },
    )

    mapping = pl.read_csv(
        mapping_path,
        schema_overrides={
            "STCOUNTY": pl.String,
            "LEAID": pl.String,
        },
    )

    validate_columns(
        closures,
        set(CLOSURE_SCHEMA),
        source=str(closures_path),
    )
    validate_columns(
        mapping,
        {"STCOUNTY", "LEAID"},
        source=str(mapping_path),
    )

    closures = (
        closures
        .with_row_index("_row_id")
        .with_columns(
            pl.col("geography")
            .str.strip_chars()
            .str.to_lowercase(),

            pl.col("state")
            .str.strip_chars()
            .str.zfill(2),

            pl.col("county")
            .str.strip_chars()
            .str.zfill(3),

            pl.col("school_district")
            .str.strip_chars(),

            pl.col("start_time").cast(pl.Float64, strict=True),
            pl.col("end_time").cast(pl.Float64, strict=True),
        )
    )

    mapping = (
        mapping
        .select(
            pl.col("STCOUNTY")
            .str.strip_chars()
            .str.zfill(5),

            pl.col("LEAID")
            .str.strip_chars(),
        )
        .filter(
            pl.col("STCOUNTY").is_not_null()
            & pl.col("LEAID").is_not_null()
            & (pl.col("STCOUNTY") != "")
            & (pl.col("LEAID") != "")
        )
        .unique()
    )

    supported_geographies = {"school district", "county", "state"}

    unsupported = (
        closures
        .filter(
            ~pl.col("geography").is_in(supported_geographies)
            | pl.col("geography").is_null()
        )
        .select("geography")
        .unique()
        .to_series()
        .to_list()
    )

    if unsupported:
        warnings.warn(
            "Ignoring unsupported geography values: "
            + ", ".join(repr(value) for value in unsupported),
            stacklevel=2,
        )

    # Rows already associated with a school district need no expansion.
    district_rows = (
        closures
        .filter(pl.col("geography") == "school district")
        .select(
            "_row_id",
            pl.lit("school district").alias("geography"),
            "state",
            "county",
            "school_district",
            "start_time",
            "end_time",
        )
    )

    # Construct the five-digit state/county mapping key.
    county_rows = (
        closures
        .filter(pl.col("geography") == "county")
        .with_columns(
            (
                pl.col("state") + pl.col("county")
            ).alias("_county_key")
        )
    )

    unmatched_counties = (
        county_rows
        .join(
            mapping,
            left_on="_county_key",
            right_on="STCOUNTY",
            how="anti",
        )
        .get_column("_county_key")
        .unique()
        .sort()
        .to_list()
    )

    if unmatched_counties:
        warnings.warn(
            "No school districts found for counties: "
            + ", ".join(unmatched_counties),
            stacklevel=2,
        )

    # An inner join expands each county row to one row per LEAID.
    expanded_county_rows = (
        county_rows
        .join(
            mapping,
            left_on="_county_key",
            right_on="STCOUNTY",
            how="inner",
            validate="m:m",
        )
        .select(
            "_row_id",
            pl.lit("school district").alias("geography"),
            "state",
            "county",
            pl.col("LEAID").alias("school_district"),
            "start_time",
            "end_time",
        )
    )

    state_rows = (
        closures
        .filter(pl.col("geography") == "state")
        .select(
            "_row_id",
            pl.lit("state").alias("geography"),
            "state",
            pl.lit(None, dtype=pl.String).alias("county"),
            pl.lit(None, dtype=pl.String).alias(
                "school_district"
            ),
            "start_time",
            "end_time",
        )
    )

    return (
        pl.concat(
            [
                district_rows,
                expanded_county_rows,
                state_rows,
            ],
            how="vertical",
        )
        .sort("_row_id")
        .select(OUTPUT_COLUMNS)
        .cast(CLOSURE_SCHEMA)
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Preprocess school closure data and expand county "
            "closures by school district."
        )
    )
    parser.add_argument(
        "--closures",
        "-i",
        type=Path,
        required=True,
        help="Path to the school closure CSV file.",
    )
    parser.add_argument(
        "--mapping",
        "-m",
        type=Path,
        required=True,
        help="Path to the school district mapping CSV file.",
    )
    parser.add_argument(
        "--output",
        "-o",
        type=Path,
        required=True,
        help="Path to the output CSV file.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    try:
        result = preprocess_school_closures(
            args.closures,
            args.mapping,
        )
        result.write_csv(args.output)
    except (OSError, ValueError, pl.exceptions.PolarsError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Wrote {result.height:,} rows to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
