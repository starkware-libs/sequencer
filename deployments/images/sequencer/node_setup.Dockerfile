# deployments/images/sequencer/Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/

FROM ghcr.io/starkware-libs/sequencer/base:latest AS planner
WORKDIR /app
COPY . .
# Installing rust version in rust-toolchain.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM ghcr.io/starkware-libs/sequencer/base:latest AS builder
WORKDIR /app
RUN curl -L https://github.com/foundry-rs/foundry/releases/download/v1.5.1/foundry_v1.5.1_linux_amd64.tar.gz | tar -xz --wildcards 'anvil'
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json --bin sequencer_node_setup
COPY . .
RUN cargo build --bin sequencer_node_setup

# Pre-warm the Cairo 1 compilation cache: sequencer_node_setup compiles its
# feature contracts on demand, so run it here (full source tree + toolchain
# present) to populate target/blockifier_test_artifacts, which the final stage
# copies so the slim runtime image needs no compiler. The args mirror the
# runtime invocation so the same cache keys are produced; the db/config output
# is discarded.
# PATH includes /app so the bootstrap step finds the anvil binary fetched above.
RUN PATH="/app:$PATH" ./target/debug/sequencer_node_setup \
    --output-base-dir /tmp/cache_warmup_output \
    --data-prefix-path /tmp/cache_warmup_data \
    --n-distributed 0 --n-hybrid 0 --n-consolidated 1 \
    && rm -rf /tmp/cache_warmup_output /tmp/cache_warmup_data

FROM ubuntu:24.04 AS final_stage

ENV ID=1001
WORKDIR /app
# Required crate for sequencer_node_setup to work
COPY --from=builder /app/crates/blockifier_test_utils/resources ./crates/blockifier_test_utils/resources
# Libfuncs allow-list read during Cairo 1 cache lookups; lives outside
# blockifier_test_utils/resources, so copy it explicitly.
COPY --from=builder /app/crates/apollo_compile_to_casm/src/allowed_libfuncs.json ./crates/apollo_compile_to_casm/src/allowed_libfuncs.json
# Pre-warmed Cairo 1 compilation cache (see builder stage).
COPY --from=builder /app/target/blockifier_test_artifacts ./target/blockifier_test_artifacts
COPY --from=builder /app/target/debug/sequencer_node_setup ./target/debug/sequencer_node_setup
COPY --from=builder /usr/bin/tini /usr/bin/tini
COPY --from=builder /app/anvil /usr/bin/anvil

# Create a new user "sequencer".
RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer; \
    mkdir -p /data /config; \
    chown -R sequencer:sequencer /app /data /config

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/debug/sequencer_node_setup"]
