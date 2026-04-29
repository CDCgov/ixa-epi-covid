STATE ?=WY
SIZE ?= 1000

# Normalize SIZE to use underscores (e.g., 1000 -> 1_000, 1000000 -> 1_000_000)
NORMALIZED_SIZE := $(shell python3 -c "print(f'{int(\"$(SIZE)\".replace(\"_\",\"\")):_}')")
CLEAN_STATE_PATTERN := $(if $(filter command line environment environment override,$(origin STATE)),$(STATE),*)
CLEAN_SIZE_PATTERN := $(if $(filter command line environment environment override,$(origin SIZE)),$(NORMALIZED_SIZE),*)

.PHONY: all help test lint format-check typecheck check uv-sync uv-sync-cloud
.PHONY: synthetic-population test-syn-pop clean-synthetic-population
.PHONY: run run-small run-large run-xl profile profile-small profile-large profile-xl
.PHONY: bench bench-compare build-rust-release docker-build-cloud-image
.PHONY: calibrate-phase-1 calibrate-phase-1-docker calibrate-phase-1-cloud
.PHONY: projections-phase-1 plot-phase-1-projection
.PHONY: calibrate-phase-1-smc projections-phase-1-smc plot-phase-1-projection-smc
.PHONY: calibrate-phase-1-dev projections-phase-1-dev plot-phase-1-projection-dev
all: uv-sync

help:
	@printf '%s\n' \
		'Available targets:' \
		'  all                         Initialize the default uv environment' \
		'  test                        Run the Python test suite' \
		'  lint                        Run ruff checks' \
		'  format-check                Check Python formatting' \
		'  typecheck                   Run ty type checks' \
		'  check                       Run lint, format check, type check, and tests' \
		'  uv-sync                     Initialize the uv environment for local development' \
		'  uv-sync-cloud               Initialize uv with Azure/cloud dependencies' \
		'  synthetic-population        Generate a synthetic population (STATE=..., SIZE=...)' \
		'  test-syn-pop                Run synthetic population generator tests' \
		'  clean-synthetic-population  Remove generated synthetic population CSVs' \
		'  run                         Run the model with a synthetic population' \
		'  run-small                   Run the model with SIZE=10_000' \
		'  run-large                   Run the model with SIZE=1_000_000' \
		'  run-xl                      Run the model with SIZE=10_000_000' \
		'  profile                     Profile the model with samply' \
		'  profile-small               Profile with SIZE=10_000' \
		'  profile-large               Profile with SIZE=1_000_000' \
		'  profile-xl                  Profile with SIZE=10_000_000' \
		'  bench                       Run infection loop benchmarks' \
		'  bench-compare               Compare benchmarks against BASE (default: HEAD)' \
		'  build-rust-release          Build the Rust binary in release mode' \
		'  docker-build-cloud-image    Build the local cloud task image' \
		'  calibrate-phase-1           Run phase-1 production calibration' \
		'  calibrate-phase-1-docker    Run phase-1 calibration via local Docker MRP' \
		'  calibrate-phase-1-cloud     Run phase-1 calibration via Azure/cloud MRP' \
		'  projections-phase-1         Run phase-1 projections' \
		'  plot-phase-1-projection     Plot phase-1 projections' \
		'  calibrate-phase-1-smc       Run phase-1 SMC calibration' \
		'  projections-phase-1-smc     Run phase-1 SMC projections' \
		'  plot-phase-1-projection-smc Plot phase-1 SMC projections' \
		'  calibrate-phase-1-dev       Run phase-1 dev calibration' \
		'  projections-phase-1-dev     Run phase-1 dev projections' \
		'  plot-phase-1-projection-dev Plot phase-1 dev projections' \
		'' \
		'Variables:' \
		'  STATE=WY                    Synthetic population state' \
		'  SIZE=1000                   Synthetic population size' \
		'  BASE=HEAD                   Benchmark comparison base ref' \
		'  MAX_WORKERS=4               Calibration/projection worker count'

# Run the Python test suite
test:
	uv run pytest

lint:
	uv run --with ruff ruff check --line-length 79 .

format-check:
	uv run --with ruff ruff format --check --line-length 79 .

typecheck:
	uv run --with ty ty check --ignore=unresolved-import

check: lint format-check typecheck test ## Run lint, format check, type check, and tests.

# Initialize the uv environment for Python scripts
uv-sync:
	uv sync --all-packages --all-extras --dev --locked

# Initialize the uv environment including Azure/cloud dependencies
uv-sync-cloud:
	uv sync --all-packages --all-extras --dev --group cloudops --locked

# Generate a synthetic population (configure with STATE and SIZE)
synthetic-population:
	uv run python -m create_synthetic_population.run --state $(STATE) --size $(NORMALIZED_SIZE)

# Run synthetic population generator tests
test-syn-pop:
	uv run pytest packages/create_synthetic_population/tests/test_create_synthetic_population.py

# Remove generated synthetic population CSVs. Optionally narrow with STATE and/or SIZE.
clean-synthetic-population:
	rm -f input/synth_pop_people_$(CLEAN_STATE_PATTERN)_$(CLEAN_SIZE_PATTERN).csv
	rm -f input/synth_pop_region_$(CLEAN_STATE_PATTERN)_$(CLEAN_SIZE_PATTERN).csv

input/synth_pop_people_%.csv:
	uv run python -m create_synthetic_population.run --state $(shell echo "$*" | sed 's/_.*//') --size $(shell echo "$*" | sed 's/^[A-Z]*_//')

# Run the model with a synthetic population (e.g., make run SIZE=1_000_000)
# Generates the population file if it doesn't exist.
run: input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv
	cargo run --release --features profiling -- -c input/input.json -o output -f --synth-population input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv

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

MAX_WORKERS ?= 4
AUTO_SIZE ?=
PHASE1_PROD_CONFIG ?= ./experiments/phase1/input/prod-config.yaml
PHASE1_DEV_CONFIG ?= ./experiments/phase1/input/dev-config.yaml
CLOUD_ARTIFACTS_DIR ?= ./experiments/phase1/calibration/cloud_artifacts
TARGET_RESULTS = ./experiments/phase1/calibration/output_indiana/results.pkl

build-rust-release:
	uv run cargo build -r

# Build the cloud task image locally. This is the image used by the docker-backed
# MRP config and the same Dockerfile the cloud runner uploads to Azure.
docker-build-cloud-image:
	docker build -t ixa-epi-covid-cloud:latest -f Dockerfile.cloud .

calibrate-phase-1: $(TARGET_RESULTS)
$(TARGET_RESULTS): ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	$(MAKE) build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c $(PHASE1_PROD_CONFIG) -o ./experiments/phase1/calibration/output_indiana --max-workers $(MAX_WORKERS)

# Run phase-1 calibration through the local Docker-backed MRP path. The Python
# wrapper still orchestrates calibration, but each particle evaluation runs
# through the cloud-task container image.
calibrate-phase-1-docker: docker-build-cloud-image
	uv run python ./scripts/phase_1_calibration.py -c $(PHASE1_PROD_CONFIG) -o ./experiments/phase1/calibration/output_indiana_docker --max-workers $(MAX_WORKERS) --docker

# Run phase-1 calibration through the Azure/cloud-backed MRP path. This target
# requires the cloudops dependency group, so bootstrap with uv-sync-cloud first.
calibrate-phase-1-cloud: uv-sync-cloud build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c $(PHASE1_PROD_CONFIG) -o ./experiments/phase1/calibration/output_indiana_cloud --max-workers $(MAX_WORKERS) --cloud $(AUTO_SIZE) --artifacts-dir $(CLOUD_ARTIFACTS_DIR) --repo-root . --dockerfile ./Dockerfile.cloud

projections-phase-1: $(TARGET_RESULTS)
	uv run python ./scripts/phase_1_projection.py -d output_indiana --max-workers $(MAX_WORKERS)

plot-phase-1-projection:
	uv run python ./scripts/plot_phase_1_projection.py -d output_indiana

calibrate-phase-1-smc: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	$(MAKE) build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-smc-config.yaml -o ./experiments/phase1/calibration/smc --max-workers $(MAX_WORKERS)

projections-phase-1-smc: ./experiments/phase1/calibration/smc/results.pkl
	uv run python ./scripts/phase_1_projection.py -d smc --max-workers $(MAX_WORKERS)

plot-phase-1-projection-smc: projections-phase-1-smc
	uv run python ./scripts/plot_phase_1_projection.py -d smc

calibrate-phase-1-dev: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	$(MAKE) build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c $(PHASE1_DEV_CONFIG) -o ./experiments/phase1/calibration/dev_$(NORMALIZED_SIZE) --default-population-size-dev $(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS)

projections-phase-1-dev: ./experiments/phase1/calibration/dev_$(NORMALIZED_SIZE)/results.pkl
	uv run python ./scripts/phase_1_projection.py -d dev_$(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS) -f --plot-distances

plot-phase-1-projection-dev: projections-phase-1-dev
	uv run python ./scripts/plot_phase_1_projection.py -d dev_$(NORMALIZED_SIZE)
