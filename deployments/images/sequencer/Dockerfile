#syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

# Compile the sequencer_node crate in release mode, ensuring dependencies are locked.
FROM base AS builder
WORKDIR /app
COPY . .
RUN cargo build --bin starknet_sequencer_node

FROM ubuntu:24.04

WORKDIR /app
COPY --from=builder /app/target/debug/starknet_sequencer_node ./target/debug/starknet_sequencer_node
COPY --from=builder /usr/bin/tini /usr/bin/tini

# Copy sequencer config
COPY config/sequencer/ /app/config/sequencer/

# Default storage /data and set permissions.
RUN mkdir /data && chown -R ubuntu:ubuntu /app /data

# Expose RPC and monitoring ports.
EXPOSE 8080 8081 8082

# Switch to ubuntu user.
USER ubuntu

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/debug/starknet_sequencer_node"]
