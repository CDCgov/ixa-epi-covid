STATE ?=WY
SIZE ?= 1000

# Normalize SIZE to use underscores (e.g., 1000 -> 1_000, 1000000 -> 1_000_000)
NORMALIZED_SIZE := $(shell python3 -c "print(f'{int(\"$(SIZE)\".replace(\"_\",\"\")):_}')")

.PHONY: all uv-sync synthetic-population run profile
all: uv-sync

# Initialize the uv environment for Python scripts
uv-sync:
	uv sync --all-packages --all-extras --dev --locked

# Generate a synthetic population (configure with STATE and SIZE)
synthetic-population:
	uv run python -m create_synthetic_population.run --state $(STATE) --size $(NORMALIZED_SIZE)

input/synth_pop_people_%.csv:
	uv run scripts/create_synthetic_population.py --state $(shell echo "$*" | sed 's/_.*//')  --size $(shell echo "$*" | sed 's/^[A-Z]*_//')

# Run the model with a synthetic population (e.g., make run SIZE=1_000_000)
# Generates the population file if it doesn't exist.
run: input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv
	cargo run --release --features profiling -- -c input/input.json -o output --no-stats -f --synth-population input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv

run-small:
	make run SIZE=10_000

run-large:
	make run SIZE=1_000_000

run-xl:
	make run SIZE=10_000_000


# Profile the model with samply (e.g., make profile SIZE=1_000_000)
profile: input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv
	cargo build --profile profiling
	samply record target/profiling/ixa-epi-covid -c input/input.json -o output --no-stats -f -v --synth-population input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv

profile-small:
	make profile SIZE=10_000

profile-large:
	make profile SIZE=1_000_000

profile-xl:
	make profile SIZE=10_000_000

# Run benchmarks
bench: input/synth_pop_people_WY_10_000.csv input/synth_pop_people_WY_100_000.csv
	cargo bench --bench infection_loop

# Compare benchmarks against a base ref (default: HEAD, i.e. uncommitted changes)
# Usage: make bench-compare              # uncommitted changes vs last commit
#        make bench-compare BASE=main    # working tree vs main
BASE ?= HEAD
bench-compare: input/synth_pop_people_WY_10_000.csv input/synth_pop_people_WY_100_000.csv
	uv run scripts/bench_compare.py $(BASE)

TARGET_RESULTS = ./experiments/phase1/calibration/output_indiana/results.pkl
calibrate-phase-1: $(TARGET_RESULTS)
$(TARGET_RESULTS): ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	uv run cargo build -r
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-config.yaml -o ./experiments/phase1/calibration/output_indiana

projections-phase-1: $(TARGET_RESULTS)
	uv run python ./scripts/phase_1_projection.py -d output_indiana

plot-phase-1-projection:
	uv run python ./scripts/plot_phase_1_projection.py -d output_indiana

calibrate-phase-1-smc: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	uv run cargo build -r
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-smc-config.yaml -o ./experiments/phase1/calibration/smc

projections-phase-1-smc: ./experiments/phase1/calibration/smc/results.pkl
	uv run python ./scripts/phase_1_projection.py -d smc

plot-phase-1-projection-smc: projections-phase-1-smc
	uv run python ./scripts/plot_phase_1_projection.py -d smc

calibrate-phase-1-dev: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	uv run cargo build -r
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/dev-config.yaml -o ./experiments/phase1/calibration/dev_$(SIZE) --default-population-size-dev $(SIZE)

projections-phase-1-dev: ./experiments/phase1/calibration/dev_$(SIZE)/results.pkl
	uv run python ./scripts/phase_1_projection.py -d dev_$(SIZE) -f --plot-distances

plot-phase-1-projection-dev: projections-phase-1-dev
	uv run python ./scripts/plot_phase_1_projection.py -d dev_$(SIZE)
