STATE ?=WY
N ?= 1000

.PHONY: all uv-sync synthetic-population
all: uv-sync

uv-sync:
	uv sync --all-packages --all-extras --dev --locked

synthetic-population:
	Rscript scripts/create_synthetic_population.R $(STATE) $(N)
