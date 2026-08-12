import sys
import warnings
from pathlib import Path

import polars as pl

CLOSURES_CSV = Path("input/school_closure_test.csv")
LEA_COUNTY_MAPPING_CSV = Path("input/grf25_lea_county.csv")
LEA_TRACT_MAPPING_CSV = Path("input/grf25_lea_tract.csv")
OUTPUT_CSV = Path("input/school_closure_test_expanded.csv")

LEA_FIELD = "LEAID"
TRACT_FIELD = "TRACT"
COUNTY_COLUMN = "county"
TRACT_COLUMN = "census_tract"
ROW_COLUMN = "__input_row"

CLOSURE_COLUMNS = [
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
    frame: pl.DataFrame, required: set[str], *, source: str
) -> None:
    missing = required.difference(frame.columns)
    if missing:
        raise ValueError(
            f"{source} is missing required columns: {', '.join(sorted(missing))}"
        )


def preprocess_school_closures(
    closures_path: Path, mapping_path: Path
) -> pl.DataFrame:
    """Expand county closures to the school districts they contain."""
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
        schema_overrides={"STCOUNTY": pl.String, "LEAID": pl.String},
    )
    validate_columns(closures, set(CLOSURE_SCHEMA), source=str(closures_path))
    validate_columns(mapping, {"STCOUNTY", "LEAID"}, source=str(mapping_path))

    closures = closures.with_row_index("_row_id").with_columns(
        pl.col("geography").str.strip_chars().str.to_lowercase(),
        pl.col("state").str.strip_chars().str.zfill(2),
        pl.col("county").str.strip_chars().str.zfill(3),
        pl.col("school_district").str.strip_chars(),
        pl.col("start_time").cast(pl.Float64, strict=True),
        pl.col("end_time").cast(pl.Float64, strict=True),
    )
    mapping = (
        mapping.select(
            pl.col("STCOUNTY").str.strip_chars().str.zfill(5),
            pl.col("LEAID").str.strip_chars(),
        )
        .filter(
            pl.col("STCOUNTY").is_not_null()
            & pl.col("LEAID").is_not_null()
            & (pl.col("STCOUNTY") != "")
            & (pl.col("LEAID") != "")
        )
        .unique()
    )

    supported = {"school district", "county", "state"}
    unsupported = (
        closures.filter(
            ~pl.col("geography").is_in(supported) | pl.col("geography").is_null()
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

    district_rows = closures.filter(
        pl.col("geography") == "school district"
    ).select(
        "_row_id",
        pl.lit("school district").alias("geography"),
        "state",
        "county",
        "school_district",
        "start_time",
        "end_time",
    )
    county_rows = closures.filter(pl.col("geography") == "county").with_columns(
        (pl.col("state") + pl.col("county")).alias("_county_key")
    )
    unmatched_counties = (
        county_rows.join(
            mapping, left_on="_county_key", right_on="STCOUNTY", how="anti"
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

    expanded_county_rows = (
        county_rows.join(
            mapping,
            left_on="_county_key",
            right_on="STCOUNTY",
            how="inner",
        ).select(
            "_row_id",
            pl.lit("school district").alias("geography"),
            "state",
            "county",
            pl.col("LEAID").alias("school_district"),
            "start_time",
            "end_time",
        )
    )
    state_rows = closures.filter(pl.col("geography") == "state").select(
        "_row_id",
        pl.lit("state").alias("geography"),
        "state",
        pl.lit(None, dtype=pl.String).alias("county"),
        pl.lit(None, dtype=pl.String).alias("school_district"),
        "start_time",
        "end_time",
    )
    return (
        pl.concat([district_rows, expanded_county_rows, state_rows], how="vertical")
        .sort("_row_id")
        .select(CLOSURE_COLUMNS)
        .cast(CLOSURE_SCHEMA)
    )


def normalized_code(column: str, width: int) -> pl.Expr:
    cleaned = (
        pl.col(column).cast(pl.String).str.strip_chars().str.replace(r"\.0+$", "")
    )
    return (
        pl.when(cleaned.str.contains(r"^\d+$").fill_null(False))
        .then(cleaned.str.zfill(width))
        .otherwise(cleaned)
    )


def validate_code_column(
    frame: pl.DataFrame, column: str, width: int, source_name: str
) -> None:
    valid = pl.col(column).str.contains(rf"^\d{{{width}}}$").fill_null(False)
    invalid = frame.filter(~valid).select(column).head(5).get_column(column).to_list()
    if invalid:
        values = ", ".join(repr(value) for value in invalid)
        raise ValueError(
            f"{source_name}: {column} must contain numeric IDs no longer than "
            f"{width} digits; invalid value(s): {values}"
        )


def load_relationships(mapping_path: Path) -> pl.DataFrame:
    raw = pl.read_csv(mapping_path, infer_schema=False)
    relationships = raw.select(
        normalized_code(LEA_FIELD, 7).alias("school_district"),
        normalized_code(TRACT_FIELD, 11).alias(TRACT_COLUMN),
    )
    validate_code_column(relationships, "school_district", 7, str(mapping_path))
    validate_code_column(relationships, TRACT_COLUMN, 11, str(mapping_path))
    return relationships.unique()


def school_district_mask() -> pl.Expr:
    normalized = (
        pl.col("geography")
        .fill_null("")
        .str.strip_chars()
        .str.to_lowercase()
        .str.replace_all(r"[\s_-]+", " ")
    )
    return normalized.is_in(["school district", "schooldistrict", "lea"])

def expand_frame(
    input_frame: pl.DataFrame, relationships: pl.DataFrame
) -> tuple[pl.DataFrame, int]:
    validate_columns(
        input_frame, {"geography", "school_district"}, source="closure output"
    )
    if TRACT_COLUMN in input_frame.columns:
        input_frame = input_frame.drop(TRACT_COLUMN)
    output_columns = [column for column in input_frame.columns if column not in [ROW_COLUMN, COUNTY_COLUMN]]
    output_columns.append(TRACT_COLUMN)

    district_rows = input_frame.filter(school_district_mask()).with_columns(
        normalized_code("school_district", 7).alias("school_district")
    )
    validate_code_column(district_rows, "school_district", 7, "closure output")
    other_rows = input_frame.filter(~school_district_mask()).with_columns(
        pl.lit(None, dtype=pl.String).alias(TRACT_COLUMN)
    )
    mapped_leas = relationships.select("school_district").unique()
    missing_rows = district_rows.join(mapped_leas, on="school_district", how="anti")
    missing_leas = (
        missing_rows.get_column("school_district").unique().sort().to_list()
        if missing_rows.height
        else []
    )
    if missing_leas:
        raise ValueError(
            "LEA IDs not present in the NCES mapping: " + ", ".join(missing_leas)
        )

    expanded = (
        district_rows.join(
            relationships,
            on="school_district",
            how="inner",
        ).with_columns(
            pl.lit("census_tract").alias("geography"),
            pl.col(TRACT_COLUMN).str.slice(0, 2).alias("state"),
            pl.col(TRACT_COLUMN).str.slice(2, 3).alias("county"),
        )
    )
    output = (
        pl.concat([expanded, other_rows], how="diagonal_relaxed")
        .sort([ROW_COLUMN, TRACT_COLUMN], nulls_last=True)
        .unique(subset=[TRACT_COLUMN])
        .select(output_columns)
        .drop("school_district")
    )
    return output, district_rows.height


def main() -> int:
    try:
        # Step 1: transform counties to school districts.
        closure_frame = preprocess_school_closures(
            CLOSURES_CSV, LEA_COUNTY_MAPPING_CSV
        ).with_row_index(ROW_COLUMN)
        print(closure_frame)
        # Step 2: generate tracts from school districts.
        relationships = load_relationships(LEA_TRACT_MAPPING_CSV)
        output, expanded_count = expand_frame(closure_frame, relationships)
        OUTPUT_CSV.parent.mkdir(parents=True, exist_ok=True)
        output.write_csv(OUTPUT_CSV, null_value="")
    except (OSError, ValueError, pl.exceptions.PolarsError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(
        f"Expanded {expanded_count} school-district rows; "
        f"wrote {output.height} rows to {OUTPUT_CSV}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
