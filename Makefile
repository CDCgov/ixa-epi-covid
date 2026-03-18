STATE ?=WY
N ?= 1000
PROFILE_SIZE ?= 1m

.PHONY: all uv-sync synthetic-population run run-1m run-10m profile setup-r
all: uv-sync

# Initialize the uv environment for Python scripts
uv-sync:
	uv sync --all-packages --all-extras --dev --locked

# Generate a synthetic population (configure with STATE and N)
synthetic-population:
	Rscript scripts/create_synthetic_population.R $(STATE) $(N)

# Run the model with the default config
run:
	cargo run --release --features profiling -- -c input/input.json -o output --no-stats -f

# Generate a 1M WY population (if needed) and run the model with it
run-1m: input/synth_pop_people_WY_1000000.csv
	cargo run --release --features profiling -- -c input/input.json -o output --no-stats -f --synth-population input/synth_pop_people_WY_1000000.csv

# Only runs if the file doesn't already exist
input/synth_pop_people_WY_1000000.csv:
	Rscript scripts/create_synthetic_population.R WY 1000000

# Generate a 10M WY population (if needed) and run the model with it
run-10m: input/synth_pop_people_WY_10000000.csv
	cargo run --release --features profiling -- -c input/input.json -o output --no-stats -f --synth-population input/synth_pop_people_WY_10000000.csv

# Only runs if the file doesn't already exist
input/synth_pop_people_WY_10000000.csv:
	Rscript scripts/create_synthetic_population.R WY 10000000

PROFILE_N_1m = 1000000
PROFILE_N_10m = 10000000

# Profile the model with samply (PROFILE_SIZE=1m or 10m)
profile: input/synth_pop_people_WY_$(PROFILE_N_$(PROFILE_SIZE)).csv
	cargo build --profile profiling
	samply record target/profiling/ixa-epi-covid -c input/input.json -o output --no-stats -f --synth-population input/synth_pop_people_WY_$(PROFILE_N_$(PROFILE_SIZE)).csv

# Install required R packages for synthetic population generation
setup-r:
	Rscript -e 'install.packages(c("tidyverse", "tigris", "sf", "tidycensus", "patchwork", "data.table"), repos="https://cloud.r-project.org")'
