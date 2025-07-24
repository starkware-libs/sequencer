# syntax = devthefuture/dockerfile-x
#
# Run with the project root context:
# docker build -f crates/apollo_network/src/bin/broadcast_network_stress_test_node/cluster/Dockerfile .
#
# For time synchronization, you can either:
# 1. Use the built-in chrony service (configured below), or
# 2. Mount host time at runtime with: -v /etc/localtime:/etc/localtime:ro -v /etc/timezone:/etc/timezone:ro

INCLUDE deployments/images/base/Dockerfile

# --- Stage 1: Install cargo-chef and prepare recipe ---
FROM base AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Stage 2: Build dependencies ---
FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin broadcast_network_stress_test_node

# --- Final Stage: Runtime image ---
FROM ubuntu:24.04 AS final_stage
COPY --from=builder /app/target/release/broadcast_network_stress_test_node /usr/local/bin/broadcast_network_stress_test_node
COPY --from=builder /app/crates/apollo_network/src/bin/broadcast_network_stress_test_node/run/entrypoint.sh /entrypoint.sh

RUN apt update && apt -y install iproute2 kmod chrony && apt clean && rm -rf /var/lib/apt/lists/*

RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
