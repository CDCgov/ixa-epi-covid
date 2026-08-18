# LEA-to-Census-Tract JSON Converter

`lea_to_tract.py` converts a CSV containing school district and census tract relationships into a JSON lookup. Each school district LEA ID becomes a JSON key, and its value is a list of the census tracts associated with that district.

The script uses [Polars](https://pola.rs/) for reading, cleaning, deduplicating, grouping, and sorting the CSV data.

## Requirements

- Python 3.9 or newer
- Polars

Install Polars with:

```powershell
python -m pip install polars
```

## Input CSV

By default, the CSV must contain these columns:

| Column | Description |
| --- | --- |
| `LEAID` | School district LEA identifier |
| `TRACT` | Census tract identifier |

Other columns may be present; the script ignores them.

Example:

```csv
LEAID,NAME_LEA25,TRACT
0100001,Fort Rucker School District,01031010300
0100001,Fort Rucker School District,01045020000
0100003,Maxwell AFB School District,01101000900
```

## Usage

From the directory containing the script:

```powershell
python lea_tracts_to_json.py INPUT_CSV [OUTPUT_JSON]
```

For example:

```powershell
python lea_tracts_to_json.py "C:\data\grf25_lea_tract.csv" "C:\data\lea_tracts.json"
```

If `OUTPUT_JSON` is omitted, the output is written beside the input CSV with `_lea_tracts` added to its filename. For example:

```powershell
python lea_tracts_to_json.py "C:\data\grf25_lea_tract.csv"
```

This creates:

```text
C:\data\grf25_lea_tract_lea_tracts.json
```

## Output

The output is a JSON object in this format:

```json
{
  "0100001": [
    "01031010300",
    "01045020000"
  ],
  "0100003": [
    "01101000900"
  ]
}
```

LEA IDs and census tract IDs are intentionally stored as JSON strings. Census tracts must remain quoted because JSON numbers cannot contain leading zeros. Storing them as strings preserves the complete 11-digit tract identifier.
### Display command help

```powershell
python lea_tracts_to_json.py --help
```

## Data handling

The script:

- reads LEA and tract identifiers as strings to preserve leading zeros;
- removes whitespace from the beginning and end of identifiers;
- skips rows with a blank LEA ID or tract ID;
- removes duplicate LEA-to-tract relationships;
- sorts LEA IDs and each district's tract list for stable output; and

If the input file is missing, required columns are absent, or the CSV cannot be read, the script prints an error and exits without reporting success.
