STATE ?=WY
SIZE ?= 1000
CARGO_HOME ?= $(HOME)/.cargo
export PATH := $(CARGO_HOME)/bin:$(PATH)

# Normalize SIZE to use underscores (e.g., 1000 -> 1_000, 1000000 -> 1_000_000)
NORMALIZED_SIZE := $(shell python3 -c "print(f'{int(\"$(SIZE)\".replace(\"_\",\"\")):_}')")
CLEAN_STATE_PATTERN := $(if $(filter command line environment environment override,$(origin STATE)),$(STATE),*)
CLEAN_SIZE_PATTERN := $(if $(filter command line environment environment override,$(origin SIZE)),$(NORMALIZED_SIZE),*)


.PHONY: all help help-vars help-cloud help-aliases check uv-sync uv-sync-cloud synthetic-population test-syn-pop clean-synthetic-population \
	run run-small run-large run-xl \
	profile profile-small profile-large profile-xl \
	bench bench-compare build-rust-release docker-build-cloud-image \
	calibrate-phase-1 calibrate-phase-1-docker calibrate-phase-1-cloud \
	cloud-list cloud-cleanup-preview cloud-cleanup-session cloud-cleanup-plan cloud-cleanup-delete \
	cloud-cleanup cloud-cleanup-user-preview cloud-cleanup-user-delete cloud-cleanup-user-plan cloud-cleanup-user-pln cloud-cleanup-user \
	projections-phase-1 plot-phase-1-projection \
	calibrate-phase-1-smc projections-phase-1-smc plot-phase-1-projection-smc \
	calibrate-phase-1-dev projections-phase-1-dev plot-phase-1-projection-dev

HELP_TARGET_WIDTH := 28
HELP_VAR_WIDTH := 44
HELP_VAR_NAMES := STATE SIZE MAX_WORKERS BASE CLOUD_CONFIG SESSION_ID CLOUD_USER DRY_RUN IMAGE_TAG SKIP_ACR AUTO_SIZE CLOUD_MAX_CONCURRENT_SIMULATIONS
HELP_VAR_STATE := Synthetic population state
HELP_VAR_SIZE := Synthetic population size; underscores are accepted
HELP_VAR_MAX_WORKERS := Calibration/projection worker count
HELP_VAR_BASE := Base ref for bench-compare
HELP_VAR_CLOUD_CONFIG := Cloud config path
HELP_VAR_SESSION_ID := Cloud session to inspect/delete
HELP_VAR_CLOUD_USER := User for cloud cleanup commands
HELP_VAR_DRY_RUN := Preview cloud cleanup without deletion
HELP_VAR_IMAGE_TAG := Optional cloud image tag cleanup selector
HELP_VAR_SKIP_ACR := Skip Azure Container Registry lookup/cleanup
HELP_VAR_AUTO_SIZE := Set to 1/true for cloud calibration auto-sizing
HELP_VAR_CLOUD_MAX_CONCURRENT_SIMULATIONS := Cloud concurrency limit
HELP_CLOUD_TARGETS := calibrate-phase-1-cloud cloud-list cloud-cleanup-preview cloud-cleanup-session cloud-cleanup-user-preview cloud-cleanup-user-delete cloud-cleanup-plan cloud-cleanup-delete cloud-cleanup cloud-cleanup-user-plan cloud-cleanup-user
HELP_ALIAS_TARGETS := run-small run-large run-xl profile-small profile-large profile-xl

all: uv-sync ## Sync the local uv environment

help: ## Show common make targets
	@awk 'BEGIN {FS = ":.*## "; printf "Usage:\n  make <target> [VAR=value]\n\nTargets:\n"} /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-$(HELP_TARGET_WIDTH)s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

help-vars: ## Show supported make variables
	@printf 'Variables:\n'
	@$(foreach var,$(HELP_VAR_NAMES),printf '  %-$(HELP_VAR_WIDTH)s %s\n' '$(var)=$(if $($(var)),$($(var)),unset)' '$(HELP_VAR_$(var))';)

help-cloud: ## Show cloud workflow details
	@printf 'Cloud:\n'
	@awk -v targets="$(HELP_CLOUD_TARGETS)" 'BEGIN {FS = ":.*## "; split(targets, target_list, /[[:space:]]+/); for (idx in target_list) wanted[target_list[idx]] = 1} /^[a-zA-Z0-9_.-]+:.*## / && wanted[$$1] {printf "  make %-$(HELP_TARGET_WIDTH)s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

help-aliases: ## Show size aliases
	@printf 'Aliases:\n'
	@awk -v targets="$(HELP_ALIAS_TARGETS)" 'BEGIN {FS = ":.*## "; split(targets, target_list, /[[:space:]]+/); for (idx in target_list) wanted[target_list[idx]] = 1} /^[a-zA-Z0-9_.-]+:.*## / && wanted[$$1] {printf "  make %-$(HELP_TARGET_WIDTH)s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# Initialize the uv environment for Python scripts
uv-sync: ## Sync uv environment with locked dependencies
	uv sync --all-packages --all-extras --dev --locked

uv-sync-cloud: ## Sync uv environment with cloud dependencies
	uv sync --all-packages --all-extras --dev --group cloudops --locked

# Run local validation equivalent to the CI checks.
check: ## Re-run pre-commit and the full test suite
	uv run python -m pre_commit run --all-files
	cargo test --verbose
	uv run pytest

# Generate a synthetic population (configure with STATE and SIZE)
synthetic-population: ## Generate input/synth_pop_* CSVs for STATE and SIZE
	uv run python -m create_synthetic_population.run --state $(STATE) --size $(NORMALIZED_SIZE)

# Run synthetic population generator tests
test-syn-pop: ## Run synthetic population generator tests
	uv run pytest packages/create_synthetic_population/tests/test_create_synthetic_population.py

# Remove generated synthetic population CSVs. Optionally narrow with STATE and/or SIZE.
clean-synthetic-population: ## Remove generated CSVs, optionally narrowed by STATE/SIZE
	rm -f input/synth_pop_people_$(CLEAN_STATE_PATTERN)_$(CLEAN_SIZE_PATTERN).csv
	rm -f input/synth_pop_region_$(CLEAN_STATE_PATTERN)_$(CLEAN_SIZE_PATTERN).csv

input/synth_pop_people_%.csv:
	uv run python -m create_synthetic_population.run --state $(shell echo "$*" | sed 's/_.*//') --size $(shell echo "$*" | sed 's/^[A-Z]*_//')

# Run the model with a synthetic population (e.g., make run SIZE=1_000_000)
# Generates the population file if it doesn't exist.
run: input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv ## Run the model with generated synthetic population
	cargo run --release --features profiling -- -c input/input.json -o output -f --synth-population input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv

run-small: ## Run with SIZE=10_000
	make run SIZE=10_000

run-large: ## Run with SIZE=1_000_000
	make run SIZE=1_000_000

run-xl: ## Run with SIZE=10_000_000
	make run SIZE=10_000_000


# Profile the model with samply (e.g., make profile SIZE=1_000_000)
profile: input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv ## Profile the model with samply
	cargo build --profile profiling
	samply record target/profiling/ixa-epi-covid -c input/input.json -o output --no-stats -f -v --synth-population input/synth_pop_people_$(STATE)_$(NORMALIZED_SIZE).csv

profile-small: ## Profile with SIZE=10_000
	make profile SIZE=10_000

profile-large: ## Profile with SIZE=1_000_000
	make profile SIZE=1_000_000

profile-xl: ## Profile with SIZE=10_000_000
	make profile SIZE=10_000_000

# Run benchmarks
bench: input/synth_pop_people_WY_10_000.csv input/synth_pop_people_WY_100_000.csv ## Run infection_loop benchmarks
	cargo bench --bench infection_loop

# Compare benchmarks against a base ref (default: HEAD, i.e. uncommitted changes)
# Usage: make bench-compare              # uncommitted changes vs last commit
#        make bench-compare BASE=main    # working tree vs main
BASE ?= HEAD
bench-compare: input/synth_pop_people_WY_10_000.csv input/synth_pop_people_WY_100_000.csv ## Compare benchmark results against BASE
	uv run scripts/bench_compare.py $(BASE)

MAX_WORKERS ?= 4
CLOUD_CONFIG ?= ixa_epi_covid.cloud_config.toml
CLOUD_USER ?=
DEFAULT_CLOUD_USER ?= $(or $(shell python3 -c 'import getpass; print(getpass.getuser())' 2>/dev/null | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9-]+/-/g; s/^-+//; s/-+$$//; s/-+/-/g'),$(firstword $(shell id -un 2>/dev/null || whoami 2>/dev/null || printf unknown)))
SESSION_ID ?=
DRY_RUN ?=
IMAGE_TAG ?=
SKIP_ACR ?=
AUTO_SIZE ?=
CLOUD_MAX_CONCURRENT_SIMULATIONS ?=
CLEANUP_USER = $(or $(strip $(CLOUD_USER)),$(DEFAULT_CLOUD_USER))
ifneq ($(strip $(SESSION_ID)),)
SESSION_ID_FLAG := --session-id "$(SESSION_ID)"
endif
ifneq ($(strip $(CLOUD_USER)),)
USER_FLAG := --user "$(CLOUD_USER)"
endif
ifneq ($(strip $(CLEANUP_USER)),)
CLEANUP_USER_FLAG := --user "$(CLEANUP_USER)"
endif
ifneq ($(strip $(DRY_RUN)),)
DRY_RUN_FLAG := --dry-run
endif
ifneq ($(strip $(IMAGE_TAG)),)
IMAGE_TAG_FLAG := --image-tag "$(IMAGE_TAG)"
endif
ifneq ($(strip $(SKIP_ACR)),)
SKIP_ACR_FLAG := --skip-acr
endif
CLOUD_AUTO_SIZE_TRUTHY = $(filter 1 true TRUE yes YES on ON --auto-size,$(AUTO_SIZE))
CLOUD_AUTO_SIZE_ARG = $(if $(CLOUD_AUTO_SIZE_TRUTHY),--auto-size,)
CLOUD_CONCURRENCY_ARG = $(if $(CLOUD_MAX_CONCURRENT_SIMULATIONS),--max-concurrent-simulations $(CLOUD_MAX_CONCURRENT_SIMULATIONS),)
CLOUD_CLEANUP_CMD = uv run python -m calibrationtools.cloud.cleanup --cloud-config "$(CLOUD_CONFIG)"
CLOUD_CLEANUP_SESSION_VARS = SESSION_ID="$(SESSION_ID)" IMAGE_TAG="$(IMAGE_TAG)" SKIP_ACR="$(SKIP_ACR)"
CLOUD_CLEANUP_USER_VARS = CLOUD_USER="$(CLEANUP_USER)" IMAGE_TAG="$(IMAGE_TAG)" SKIP_ACR="$(SKIP_ACR)"
TARGET_RESULTS = ./experiments/phase1/calibration/output_indiana/results.pkl

build-rust-release: ## Build the Rust binary in release mode
	uv run cargo build -r

docker-build-cloud-image: ## Build the cloud Docker image
	docker build -t ixa-epi-covid-cloud:latest -f Dockerfile.cloud .

calibrate-phase-1: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json ## Run production phase 1 calibration
	$(MAKE) build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-config.yaml -o ./experiments/phase1/calibration/output_indiana --default-population-size-dev $(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS)

$(TARGET_RESULTS): ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json
	$(MAKE) calibrate-phase-1

calibrate-phase-1-docker: docker-build-cloud-image ## Run production phase 1 calibration via Docker MRP
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-config.yaml -o ./experiments/phase1/calibration/output_indiana_docker --default-population-size-dev $(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS) --docker

calibrate-phase-1-cloud: uv-sync-cloud ## Run production phase 1 calibration via Azure/cloud MRP
ifneq ($(CLOUD_AUTO_SIZE_TRUTHY),)
	$(MAKE) build-rust-release
endif
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-config.yaml -o ./experiments/phase1/calibration/output_indiana_cloud --default-population-size-dev $(NORMALIZED_SIZE) --cloud --cloud-config $(CLOUD_CONFIG) $(CLOUD_AUTO_SIZE_ARG) $(CLOUD_CONCURRENCY_ARG)

cloud-list: uv-sync-cloud ## List project-scoped cloud resources
	$(CLOUD_CLEANUP_CMD) --list $(SESSION_ID_FLAG) $(USER_FLAG) $(IMAGE_TAG_FLAG) $(SKIP_ACR_FLAG)

cloud-cleanup-preview: uv-sync-cloud ## Preview cleanup for SESSION_ID; optional IMAGE_TAG/SKIP_ACR
	$(MAKE) cloud-cleanup $(CLOUD_CLEANUP_SESSION_VARS) DRY_RUN=1

cloud-cleanup-session: uv-sync-cloud ## Delete cloud resources for SESSION_ID; optional IMAGE_TAG/SKIP_ACR
	$(MAKE) cloud-cleanup $(CLOUD_CLEANUP_SESSION_VARS) DRY_RUN=

cloud-cleanup-plan: cloud-cleanup-preview ## Alias for cloud-cleanup-preview

cloud-cleanup-delete: cloud-cleanup-session ## Alias for cloud-cleanup-session

cloud-cleanup: ## Underlying SESSION_ID cleanup target
	@test -n "$(SESSION_ID)" || (echo "SESSION_ID is required"; exit 1)
	$(CLOUD_CLEANUP_CMD) $(SESSION_ID_FLAG) $(IMAGE_TAG_FLAG) $(SKIP_ACR_FLAG) $(DRY_RUN_FLAG)

cloud-cleanup-user-preview: uv-sync-cloud ## Preview cleanup for CLOUD_USER; defaults to current user
	$(MAKE) cloud-cleanup-user $(CLOUD_CLEANUP_USER_VARS) DRY_RUN=1

cloud-cleanup-user-delete: uv-sync-cloud ## Delete all cloud sessions for CLOUD_USER; defaults to current user
	$(MAKE) cloud-cleanup-user $(CLOUD_CLEANUP_USER_VARS) DRY_RUN=

cloud-cleanup-user-plan: cloud-cleanup-user-preview ## Alias for cloud-cleanup-user-preview

cloud-cleanup-user-pln: cloud-cleanup-user-preview ## Alias for cloud-cleanup-user-preview

cloud-cleanup-user: ## Underlying CLOUD_USER cleanup target
	@test -n "$(CLEANUP_USER)" || (echo "CLOUD_USER is required"; exit 1)
	$(CLOUD_CLEANUP_CMD) --all-sessions-for-user $(CLEANUP_USER_FLAG) $(IMAGE_TAG_FLAG) $(SKIP_ACR_FLAG) $(DRY_RUN_FLAG)

projections-phase-1: $(TARGET_RESULTS) ## Run production phase 1 projections
	uv run python ./scripts/phase_1_projection.py -d output_indiana --max-workers $(MAX_WORKERS)

plot-phase-1-projection: ## Plot production phase 1 projections
	uv run python ./scripts/plot_phase_1_projection.py -d output_indiana

calibrate-phase-1-smc: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json ## Run SMC phase 1 calibration
	$(MAKE) build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/prod-smc-config.yaml -o ./experiments/phase1/calibration/smc --default-population-size-dev $(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS)

projections-phase-1-smc: ./experiments/phase1/calibration/smc/results.pkl ## Run SMC phase 1 projections
	uv run python ./scripts/phase_1_projection.py -d smc --max-workers $(MAX_WORKERS)

plot-phase-1-projection-smc: projections-phase-1-smc ## Plot SMC phase 1 projections
	uv run python ./scripts/plot_phase_1_projection.py -d smc

calibrate-phase-1-dev: ./experiments/phase1/input/priors.json ./experiments/phase1/input/default_params.json ## Run dev phase 1 calibration with SIZE
	$(MAKE) build-rust-release
	uv run python ./scripts/phase_1_calibration.py -c ./experiments/phase1/input/dev-config.yaml -o ./experiments/phase1/calibration/dev_$(NORMALIZED_SIZE) --default-population-size-dev $(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS)

projections-phase-1-dev: ./experiments/phase1/calibration/dev_$(NORMALIZED_SIZE)/results.pkl ## Run dev phase 1 projections
	uv run python ./scripts/phase_1_projection.py -d dev_$(NORMALIZED_SIZE) --max-workers $(MAX_WORKERS) -f --plot-distances

plot-phase-1-projection-dev: projections-phase-1-dev ## Plot dev phase 1 projections
	uv run python ./scripts/plot_phase_1_projection.py -d dev_$(NORMALIZED_SIZE)
