# School closure tract expansion

`generate_school_closure_timeline.py` uses `pygris` to download US Census tract data, then expands closure records at the State, County, or CensusTract level. For every tract reached by at least one row, it writes the minimum `start_time` and maximum `end_time`. The user does not need to supply a geography mapping CSV.

## Install

Install Python, then install `pygris`:

```text
python -m pip install pygris
```

## Run

```text
uv run python packages/school_closure/src/school_closure/generate_school_closure_timeline.py input/school_closure_test.csv input/school_closures_enumerated_test.csv
```

Choose the Census tract vintage with `--year` when necessary:

```text
uv run python packages/school_closure/src/school_closure/generate_school_closure_timeline.py input/school_closure_test.csv input/school_closures_enumerated_test.csv --year 2024
```

The closure file must contain:

```text
geography_type,state_fips,county_fips,census_tract,start_time,end_time
```

`county_fips` is required for County and CensusTract rows.
`census_tract` is required for CensusTract rows. The script reads the states
in this file and downloads their tract records from Census TIGER/Line through
`pygris`. Downloads are cached by `pygris` for subsequent runs.

## Assumptions and behavior

- Output tract identifiers are canonical 11-digit Census tract GEOID strings: two-digit state FIPS + three-digit county FIPS + six-digit tract code.
- Identifiers are treated as strings so leading zeroes survive. A tract value may be a six-digit component, an 11-digit GEOID, or a 10-digit GEOID that
  lost the leading zero from a one-digit state FIPS. For example, sample value `6037100100` becomes `06037100100`.
- State and county rows expand to the tracts in the selected Census vintage. A closure geography with no matching tracts is treated as an error rather than silently dropped.
- The default vintage is 2024. Set `--year` to the year matching the source closure data. Tract identifiers and boundaries can change between vintages.
- Internet access is required on the first run for each state/year. Cached downloads can be reused by later runs.
- Overlapping State, County, and CensusTract rows all contribute to a tract. The result uses the smallest start and largest end across those rows.
- This min/max operation creates one overall time envelope. If closures have gaps, the output does not preserve them; representing separate intervals would require a different output schema.
- `end_time == start_time` is allowed; `end_time < start_time`, non-numeric values, and non-finite values are rejected.
- The sample Rust type `u32` cannot hold every 11-digit Census tract GEOID. Keep the full GEOID as a string, or store only the six-digit tract component alongside state and county.
