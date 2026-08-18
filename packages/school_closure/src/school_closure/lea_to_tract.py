import argparse
import json
from pathlib import Path

import polars as pl

DEFAULT_LEA_COLUMN = "LEAID"
DEFAULT_TRACT_COLUMN = "TRACT"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "input_csv", type=Path, help="Path to the input CSV file"
    )
    parser.add_argument(
        "output_json",
        type=Path,
    )
    parser.add_argument(
        "--state", type=str, default=None, help="State code to filter tracts"
    )

    return parser.parse_args()


def build_mapping(
    input_csv: Path,
    lea_column: str,
    tract_column: str,
    state: str | None = None,
) -> dict[str, list[str]]:
    """Return the LEA-to-tract mapping, skipped-row count, and duplicate count."""
    # Force both identifier columns to strings so Polars preserves leading zeroes.
    frame = pl.read_csv(
        input_csv,
        columns=[lea_column, tract_column],
        schema_overrides={lea_column: pl.String, tract_column: pl.String},
    ).with_columns(
        pl.col(lea_column).str.strip_chars(),
        pl.col(tract_column).str.strip_chars(),
    )

    blank_id = pl.any_horizontal(
        pl.col(lea_column).is_null(),
        pl.col(tract_column).is_null(),
        pl.col(lea_column) == "",
        pl.col(tract_column) == "",
    )

    frame = frame.filter(
        pl.col(tract_column).str.slice(0, 2) == state if state else True
    )

    valid = frame.filter(~blank_id)
    unique_links = valid.unique(subset=[lea_column, tract_column])

    grouped = (
        unique_links.group_by(lea_column)
        .agg(pl.col(tract_column).sort().alias("tracts"))
        .sort(lea_column)
    )

    # Only the final conversion to JSON-compatible Python objects leaves Polars.
    mapping = dict(
        zip(grouped[lea_column].to_list(), grouped["tracts"].to_list())
    )
    return mapping


def main() -> int:
    args = parse_args()
    input_csv: Path = args.input_csv
    output_json: Path = args.output_json
    state: str | None = args.state

    mapping = build_mapping(
        input_csv, DEFAULT_LEA_COLUMN, DEFAULT_TRACT_COLUMN, state
    )
    output_json.parent.mkdir(parents=True, exist_ok=True)
    with output_json.open("w", encoding="utf-8", newline="\n") as json_file:
        json.dump(mapping, json_file, ensure_ascii=False, indent=2)
        json_file.write("\n")


if __name__ == "__main__":
    main()
