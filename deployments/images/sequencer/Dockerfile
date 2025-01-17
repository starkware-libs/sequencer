#syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile


# Compile the sequencer_node crate in release mode, ensuring dependencies are locked.
FROM base AS builder
WORKDIR /app
COPY . .
RUN cargo build --bin starknet_sequencer_node

FROM base AS sequencer

ENV ID=1000
WORKDIR /app
COPY --from=builder /app/target/debug/starknet_sequencer_node ./target/debug/starknet_sequencer_node

# Copy sequencer config
COPY config/sequencer/default_config.json /app/config/sequencer/

# Create a new user "sequencer".
RUN set -ex; \
    addgroup --gid ${ID} sequencer; \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home /app sequencer; \
    chown -R sequencer:sequencer /app

# Expose RPC and monitoring ports.
EXPOSE 8080 8081 8082

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/debug/starknet_sequencer_node"]
