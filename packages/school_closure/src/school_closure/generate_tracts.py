import argparse
import csv
import io
import re
import sys
import urllib.request
import zipfile
from pathlib import Path
from typing import Iterable

import polars as pl


DEFAULT_YEAR = 2025
NCES_URL = "https://nces.ed.gov/programs/edge/data/GRF{year2}.zip"
LEA_FIELD_CANDIDATES = ("LEAID", "LEA_ID", "NCES_LEA_ID")
TRACT_FIELD_CANDIDATES = ("TRACT", "TRACTCE", "GEOID", "GEOID_TRACT")
TRACT_COLUMN = "census_tract"
OVERLAP_COLUMN = "school_district_overlap_count"
ROW_COLUMN = "__input_row"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Replace every school-district row with one row per census tract "
            "using the NCES EDGE LEA-to-tract relationship file."
        )
    )
    parser.add_argument("input_csv", type=Path)
    parser.add_argument("output_csv", type=Path)
    parser.add_argument(
        "--year",
        type=int,
        default=DEFAULT_YEAR,
        help="NCES EDGE GRF vintage to download (default: %(default)s)",
    )
    parser.add_argument(
        "--mapping",
        type=Path,
        help="Local NCES GRF ZIP or extracted LEA-to-tract CSV",
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=Path.home() / ".cache" / "nces-edge",
        help="Directory for downloaded NCES archives",
    )
    parser.add_argument(
        "--on-missing",
        choices=("error", "keep", "drop"),
        default="error",
        help="What to do with input LEA IDs absent from the mapping",
    )
    return parser.parse_args()


def download_mapping(year: int, cache_dir: Path) -> Path:
    if year < 2000 or year > 2099:
        raise ValueError("--year must be a four-digit year")

    cache_dir.mkdir(parents=True, exist_ok=True)
    destination = cache_dir / f"GRF{year % 100:02d}.zip"
    if destination.exists() and destination.stat().st_size > 0:
        return destination

    url = NCES_URL.format(year2=f"{year % 100:02d}")
    temporary = destination.with_suffix(".zip.part")
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "Python Polars NCES EDGE client"},
    )
    print(f"Downloading {url}", file=sys.stderr)
    try:
        with urllib.request.urlopen(request, timeout=120) as source:
            with temporary.open("wb") as target:
                while chunk := source.read(1024 * 1024):
                    target.write(chunk)
        temporary.replace(destination)
    except Exception:
        temporary.unlink(missing_ok=True)
        raise
    return destination


def mapping_bytes(path: Path) -> bytes:
    if path.suffix.lower() != ".zip":
        return path.read_bytes()

    with zipfile.ZipFile(path) as archive:
        candidates = [
            info
            for info in archive.infolist()
            if not info.is_dir()
            and "lea_tract" in info.filename.lower()
            and info.filename.lower().endswith((".csv", ".txt"))
        ]
        if len(candidates) != 1:
            names = ", ".join(info.filename for info in candidates) or "none"
            raise ValueError(
                f"expected one LEA-to-tract table in {path}; "
                f"found {len(candidates)}: {names}"
            )
        return archive.read(candidates[0])


def detect_separator(data: bytes) -> str:
    sample = data[:8192].decode("utf-8-sig", errors="replace")
    try:
        return csv.Sniffer().sniff(sample, delimiters=",\t|").delimiter
    except csv.Error:
        return ","


def read_csv_bytes(data: bytes) -> pl.DataFrame:
    options = {
        "separator": detect_separator(data),
        "infer_schema": False,
    }
    try:
        return pl.read_csv(io.BytesIO(data), encoding="utf8", **options)
    except pl.exceptions.ComputeError:
        return pl.read_csv(io.BytesIO(data), encoding="windows-1252", **options)


def read_input(path: Path) -> pl.DataFrame:
    return pl.read_csv(
        path,
        infer_schema=False,
    ).with_row_index(ROW_COLUMN)


def find_field(columns: Iterable[str], candidates: Iterable[str]) -> str:
    lookup = {name.strip().upper(): name for name in columns}
    for candidate in candidates:
        if candidate in lookup:
            return lookup[candidate]
    raise ValueError(
        f"mapping is missing one of {', '.join(candidates)}; "
        f"found columns: {', '.join(columns)}"
    )


def normalized_code(column: str, width: int) -> pl.Expr:
    cleaned = (
        pl.col(column)
        .cast(pl.String)
        .str.strip_chars()
        .str.replace(r"\.0+$", "")
    )
    return (
        pl.when(cleaned.str.contains(r"^\d+$").fill_null(False))
        .then(cleaned.str.zfill(width))
        .otherwise(cleaned)
    )


def validate_code_column(
    frame: pl.DataFrame,
    column: str,
    width: int,
    source_name: str,
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
    """Build a distinct LEA/tract table with the global overlap count."""
    raw = read_csv_bytes(mapping_bytes(mapping_path))
    lea_field = find_field(raw.columns, LEA_FIELD_CANDIDATES)
    tract_field = find_field(raw.columns, TRACT_FIELD_CANDIDATES)

    relationships = raw.select(
        normalized_code(lea_field, 7).alias("school_district"),
        normalized_code(tract_field, 11).alias(TRACT_COLUMN),
    )
    validate_code_column(relationships, "school_district", 7, str(mapping_path))
    validate_code_column(relationships, TRACT_COLUMN, 11, str(mapping_path))

    relationships = relationships.unique().sort(["school_district", TRACT_COLUMN])
    overlap_counts = relationships.group_by(TRACT_COLUMN).agg(
        pl.col("school_district").n_unique().alias(OVERLAP_COLUMN)
    )
    return relationships.join(
        overlap_counts,
        on=TRACT_COLUMN,
        how="left",
        validate="m:1",
    )


def school_district_mask() -> pl.Expr:
    normalized = (
        pl.col("geography")
        .fill_null("")
        .str.strip_chars()
        .str.to_lowercase()
        .str.replace_all(r"[\s_-]+", " ")
    )
    return normalized.is_in(["school district", "schooldistrict", "lea"])


def empty_geography_columns() -> list[pl.Expr]:
    return [
        pl.lit(None, dtype=pl.String).alias(TRACT_COLUMN),
        pl.lit(None, dtype=pl.UInt32).alias(OVERLAP_COLUMN),
    ]


def expand_frame(
    input_frame: pl.DataFrame,
    relationships: pl.DataFrame,
    on_missing: str,
) -> tuple[pl.DataFrame, int, list[str]]:
    required = {"geography", "school_district"}
    missing_columns = sorted(required.difference(input_frame.columns))
    if missing_columns:
        raise ValueError(f"input is missing columns: {', '.join(missing_columns)}")

    generated_columns = [TRACT_COLUMN, OVERLAP_COLUMN]
    existing_generated = [
        column for column in generated_columns if column in input_frame.columns
    ]
    if existing_generated:
        input_frame = input_frame.drop(existing_generated)
    output_columns = [column for column in input_frame.columns if column != ROW_COLUMN]
    for column in (TRACT_COLUMN, OVERLAP_COLUMN):
        if column not in output_columns:
            output_columns.append(column)

    district_rows = input_frame.filter(school_district_mask()).with_columns(
        normalized_code("school_district", 7).alias("school_district")
    )
    validate_code_column(district_rows, "school_district", 7, "input CSV")

    other_rows = input_frame.filter(~school_district_mask()).with_columns(
        empty_geography_columns()
    )
    mapped_leas = relationships.select("school_district").unique()
    missing_rows = district_rows.join(
        mapped_leas,
        on="school_district",
        how="anti",
    )
    missing_leas = (
        missing_rows.get_column("school_district").unique().sort().to_list()
        if missing_rows.height
        else []
    )

    if missing_leas and on_missing == "error":
        raise ValueError(
            "LEA IDs not present in the NCES mapping: " + ", ".join(missing_leas)
        )

    expanded = (
        district_rows.join(
            relationships,
            on="school_district",
            how="inner",
            validate="m:m",
            maintain_order="left",
        )
        .with_columns(
            pl.lit("census tract").alias("geography"),
            pl.col(TRACT_COLUMN).str.slice(0, 2).alias("state"),
            pl.col(TRACT_COLUMN).str.slice(2, 3).alias("county"),
        )
    )

    frames = [expanded, other_rows]
    if on_missing == "keep" and missing_rows.height:
        frames.append(missing_rows.with_columns(empty_geography_columns()))

    output = (
        pl.concat(frames, how="diagonal_relaxed")
        .sort([ROW_COLUMN, TRACT_COLUMN], nulls_last=True)
        .select(output_columns)
    )
    expanded_input_count = district_rows.height - missing_rows.height
    return output, expanded_input_count, missing_leas


def main() -> int:
    args = parse_args()
    try:
        mapping_path = args.mapping or download_mapping(args.year, args.cache_dir)
        relationships = load_relationships(mapping_path)
        input_frame = read_input(args.input_csv)
        output, expanded_count, missing_leas = expand_frame(
            input_frame,
            relationships,
            args.on_missing,
        )
        args.output_csv.parent.mkdir(parents=True, exist_ok=True)
        output.write_csv(args.output_csv, null_value="")
    except (OSError, ValueError, pl.exceptions.PolarsError, zipfile.BadZipFile) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(
        f"Expanded {expanded_count} school-district rows; "
        f"wrote {output.height} rows to {args.output_csv}",
        file=sys.stderr,
    )
    if missing_leas:
        print(
            f"Unmapped LEA IDs ({args.on_missing}): {', '.join(missing_leas)}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())