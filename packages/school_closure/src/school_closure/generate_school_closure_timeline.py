import argparse
import csv
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

STATE_ALIASES = ("state_fips", "STATEFP", "state", "statefp")
COUNTY_ALIASES = ("county_fips", "COUNTYFP", "county", "countyfp")
TRACT_ALIASES = (
    "census_tract",
    "GEOID",
    "GEOID10",
    "GEOID20",
    "tract_geoid",
    "TRACTCE",
    "TRACTCE10",
    "TRACTCE20",
    "tract",
)


class InputError(ValueError):
    """Raised for a malformed input row."""


@dataclass(frozen=True)
class Tract:
    geoid: str
    state_fips: str
    county_fips: str


def clean(value: object) -> str:
    return "" if value is None else str(value).strip()


def digits(value: object, field: str, row_number: int) -> str:
    result = clean(value)
    if not result or not result.isdigit():
        raise InputError(f"row {row_number}: {field} must contain digits")
    return result


def value_from(
    row: dict[str, str], aliases: Iterable[str], field: str, row_number: int
) -> str:
    for alias in aliases:
        if alias in row and clean(row[alias]):
            return clean(row[alias])
    raise InputError(
        f"row {row_number}: missing {field}; accepted columns: {', '.join(aliases)}"
    )


def optional_value(row: dict[str, str], aliases: Iterable[str]) -> str:
    for alias in aliases:
        if alias in row and clean(row[alias]):
            return clean(row[alias])
    return ""


def normalize_state(value: object, row_number: int) -> str:
    raw = digits(value, "state_fips", row_number)
    if len(raw) > 2 or int(raw) > 99:
        raise InputError(f"row {row_number}: invalid state_fips {raw!r}")
    return raw.zfill(2)


def normalize_county(value: object, row_number: int) -> str:
    raw = digits(value, "county_fips", row_number)
    if len(raw) > 3 or int(raw) > 999:
        raise InputError(f"row {row_number}: invalid county_fips {raw!r}")
    return raw.zfill(3)


def normalize_tract(
    value: object, state_fips: str, county_fips: str, row_number: int
) -> str:
    """Return an 11-digit tract GEOID.

    Accepted forms:
    - a six-digit tract code (TRACTCE), combined with state/county;
    - a canonical 11-digit GEOID;
    - a 10-digit GEOID whose one-digit state code lost its leading zero.
    """
    raw = digits(value, "census_tract", row_number)

    if len(raw) <= 6:
        geoid = state_fips + county_fips + raw.zfill(6)
    elif len(raw) == 10:
        geoid = raw.zfill(11)
    elif len(raw) == 11:
        geoid = raw
    else:
        raise InputError(
            f"row {row_number}: census_tract {raw!r} must be a tract code "
            "(up to 6 digits) or a 10/11-digit GEOID"
        )

    expected_prefix = state_fips + county_fips
    if not geoid.startswith(expected_prefix):
        raise InputError(
            f"row {row_number}: tract GEOID {geoid} conflicts with "
            f"state/county {state_fips}/{county_fips}"
        )
    return geoid


def build_mapping(
    records: Iterable[dict[str, object]],
) -> tuple[
    dict[str, Tract], dict[str, set[str]], dict[tuple[str, str], set[str]]
]:
    by_geoid: dict[str, Tract] = {}
    by_state: dict[str, set[str]] = {}
    by_county: dict[tuple[str, str], set[str]] = {}

    for row_number, row in enumerate(records, start=1):
        state = normalize_state(
            value_from(row, STATE_ALIASES, "state FIPS", row_number),
            row_number,
        )
        county = normalize_county(
            value_from(row, COUNTY_ALIASES, "county FIPS", row_number),
            row_number,
        )
        tract_raw = value_from(row, TRACT_ALIASES, "census tract", row_number)
        geoid = normalize_tract(tract_raw, state, county, row_number)
        tract = Tract(geoid, state, county)

        previous = by_geoid.get(geoid)
        if previous is not None and previous != tract:
            raise InputError(f"conflicting Census data for tract {geoid}")
        by_geoid[geoid] = tract
        by_state.setdefault(state, set()).add(geoid)
        by_county.setdefault((state, county), set()).add(geoid)

    if not by_geoid:
        raise InputError("pygris returned no census tracts")
    return by_geoid, by_state, by_county


def download_mapping(
    state_fips_codes: Iterable[str], year: int
) -> tuple[
    dict[str, Tract], dict[str, set[str]], dict[tuple[str, str], set[str]]
]:
    try:
        from pygris import tracts
    except ImportError as exc:
        raise InputError(
            "pygris is not installed; run: python -m pip install pygris"
        ) from exc

    records: list[dict[str, object]] = []
    for state in sorted(set(state_fips_codes)):
        print(f"Downloading {year} census tracts for state {state}...")
        try:
            frame = tracts(
                state=state,
                year=year,
                cb=True,
                cache=True,
            )
        except Exception as exc:
            raise InputError(
                f"could not download {year} tracts for state {state}: {exc}"
            ) from exc
        records.extend(frame.to_dict(orient="records"))

    return build_mapping(records)


def parse_time(value: object, field: str, row_number: int) -> float:
    try:
        result = float(clean(value))
    except ValueError as exc:
        raise InputError(
            f"row {row_number}: invalid {field} {value!r}"
        ) from exc
    if not math.isfinite(result):
        raise InputError(f"row {row_number}: {field} must be finite")
    return result


def format_number(value: float) -> str:
    return str(int(value)) if value.is_integer() else format(value, ".15g")


def transform(closures_path: Path, output_path: Path, year: int) -> int:
    aggregate: dict[str, tuple[float, float]] = {}

    with closures_path.open("r", encoding="utf-8-sig", newline="") as handle:
        reader = csv.DictReader(handle)
        required = {"geography_type", "state_fips", "start_time", "end_time"}
        missing = required.difference(reader.fieldnames or ())
        if missing:
            raise InputError(
                "closure input is missing column(s): "
                + ", ".join(sorted(missing))
            )
        closure_rows = list(reader)

    if not closure_rows:
        raise InputError("closure input contains no data rows")

    state_fips_codes = {
        normalize_state(row["state_fips"], row_number)
        for row_number, row in enumerate(closure_rows, start=2)
    }
    by_geoid, by_state, by_county = download_mapping(state_fips_codes, year)

    for row_number, row in enumerate(closure_rows, start=2):
        kind = clean(row["geography_type"]).casefold().replace("_", "")
        state = normalize_state(row["state_fips"], row_number)
        start = parse_time(row["start_time"], "start_time", row_number)
        end = parse_time(row["end_time"], "end_time", row_number)
        if end < start:
            raise InputError(
                f"row {row_number}: end_time {end} precedes start_time {start}"
            )

        if kind == "state":
            geoids = by_state.get(state, set())
            label = f"state {state}"
        elif kind == "county":
            county = normalize_county(row.get("county_fips"), row_number)
            geoids = by_county.get((state, county), set())
            label = f"county {state}{county}"
        elif kind in {"censustract", "tract"}:
            county = normalize_county(row.get("county_fips"), row_number)
            tract_raw = optional_value(row, TRACT_ALIASES)
            geoid = normalize_tract(tract_raw, state, county, row_number)
            geoids = {geoid} if geoid in by_geoid else set()
            label = f"tract {geoid}"
        else:
            raise InputError(
                f"row {row_number}: unsupported geography_type "
                f"{row['geography_type']!r}"
            )

        if not geoids:
            raise InputError(
                f"row {row_number}: {label} has no tracts in the "
                f"{year} Census geography"
            )

        for geoid in geoids:
            old = aggregate.get(geoid)
            aggregate[geoid] = (
                start if old is None else min(old[0], start),
                end if old is None else max(old[1], end),
            )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(("census_tract", "start_time", "end_time"))
        for geoid in sorted(aggregate):
            start, end = aggregate[geoid]
            writer.writerow((geoid, format_number(start), format_number(end)))
    return len(aggregate)


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Download Census tracts with pygris; expand State, County, and "
            "CensusTract school closures; then aggregate each tract to "
            "min(start_time) and max(end_time)."
        )
    )
    parser.add_argument("closures_csv", type=Path)
    parser.add_argument("output_csv", type=Path)
    parser.add_argument(
        "--year",
        type=int,
        default=2024,
        help="Census TIGER/Line tract vintage (default: 2024)",
    )
    args = parser.parse_args()

    try:
        count = transform(args.closures_csv, args.output_csv, args.year)
    except (OSError, InputError, csv.Error) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(f"Wrote {count} census tracts to {args.output_csv}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
