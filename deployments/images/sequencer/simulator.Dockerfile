# syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

# Compile the sequencer_node crate in release mode, ensuring dependencies are locked.
FROM base AS builder
WORKDIR /app
COPY . .
RUN cargo build --bin sequencer_simulator

FROM ubuntu:24.04 as final_stage

ENV ID=1001
WORKDIR /app
COPY --from=builder /app/target/debug/sequencer_simulator ./target/debug/sequencer_simulator
COPY --from=builder /usr/bin/tini /usr/bin/tini

COPY config/sequencer config/sequencer

# Create a new user "sequencer".
RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer; \
    mkdir /data; \
    chown -R sequencer:sequencer /app /data

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/debug/sequencer_simulator"]
