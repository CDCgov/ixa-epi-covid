.PHONY: all uv-sync
all: uv-sync

uv-sync:
	uv sync --all-packages --all-extras --dev --locked
