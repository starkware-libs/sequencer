# syntax = devthefuture/dockerfile-x
# deployments/images/sequencer/node_setup.Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/
# We use dockerfile-x, for more information visit: https://github.com/devthefuture-org/dockerfile-x/blob/master/README.md

INCLUDE deployments/images/base/Dockerfile

FROM base AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .
RUN cargo install --git https://github.com/foundry-rs/foundry anvil --locked --tag=v0.3.0
RUN cargo build

FROM ubuntu:24.04 AS final_stage

ENV ID=1001
WORKDIR /app
# Required crate for sequencer_node_setup to work
COPY --from=builder /app/crates/blockifier_test_utils/resources ./crates/blockifier_test_utils/resources
COPY --from=builder /app/target/debug/sequencer_node_setup ./target/debug/sequencer_node_setup
COPY --from=builder /usr/bin/tini /usr/bin/tini
COPY --from=builder /var/tmp/rust/bin/anvil /usr/bin/anvil

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
