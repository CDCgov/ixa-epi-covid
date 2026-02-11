.PHONY: all uv-sync maturin-dev epicovid
all: uv-sync maturin-dev epicovid

uv-sync:
	uv sync --all-packages --all-extras --dev --locked
maturin-dev:
	uv run maturin develop
epicovid: uv-sync maturin-dev
	uv run python src/pyepicovid/__main__.py
