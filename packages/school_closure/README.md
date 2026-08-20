# School Closure Pre-processing
ETL package for generating school district to FIPS census tract code mapping given Geographic Reference Files (GRF) CSV mapping of Local Education Agency (LEA) ID codes to census tract FIPS codes.

## Background
In ixa-epi-covid school closures are modeled at the school district, county and state level. School districts are designated by an LEA ID which is different numeric representation than FIPS code. Since FIPS codes are used in the model, this packages creates a mapping of school districts to the set of census tracts that fully or partially overlap with the school district. String representations are used in the script for both school district and FIPS codes to preserve leading zeros.

The required CSV file is available through [National Center for Education Statistics](https://nces.ed.gov/programs/edge/geographic/relationshipfiles).

## Getting Started
The script takes in three arguments
 - path to the GRF LEA ID to FIPS code mapping CSV
 - path the output file where the JSON will be written
 - an optional parameter of a state's FIPS Code (e.g., WY = 56)

`uv run python packages/school_closure/src/school_closure/src/lea_to_tract.py input/grf25_lea_tract.csv input/district_tract_mapping.json --state 56`
