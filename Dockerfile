# # Azure Batch worker image: the Rust model binary plus the Python pipeline.
# # The worker command comes from AzureBatchExecutor, so do not set an ENTRYPOINT.

FROM docker.io/library/rust:1-bookworm AS model-build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches
RUN cargo build --release --bin ixa-epi-covid


FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim

WORKDIR /app

COPY pyproject.toml uv.lock README.md ./
COPY src ./src
COPY scripts ./scripts
COPY packages/importation ./packages/importation
COPY packages/particle_reader ./packages/particle_reader
RUN mkdir -p ./experiments/phase2/calibration/azure
RUN mkdir -p ./experiments/phase2/input
COPY input/people_test.csv ./experiments/phase2/input/people_test.csv
RUN apt-get update \
    && apt-get install --yes --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/*

RUN uv sync --no-dev --all-packages

COPY --from=model-build /build/target/release/ixa-epi-covid ./target/release/ixa-epi-covid

ENV PATH="/app/.venv/bin:${PATH}"