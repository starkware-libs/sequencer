# syntax = devthefuture/dockerfile-x
#
# Run with the project root context:
# docker build -f crates/apollo_network/src/bin/broadcast_network_stress_test_node/cluster/Dockerfile .

INCLUDE deployments/images/base/Dockerfile

FROM base AS builder
WORKDIR /usr/src/rust_code
COPY . .
# TODO(AndrewL): use cargo chef for better caching
RUN cargo build --release --bin broadcast_network_stress_test_node

FROM ubuntu:24.04 AS final_stage
COPY --from=builder /usr/src/rust_code/target/release/broadcast_network_stress_test_node /usr/local/bin/broadcast_network_stress_test_node
COPY --from=builder /usr/src/rust_code/crates/apollo_network/src/bin/broadcast_network_stress_test_node/run/network_gater.sh /network_gater.sh
RUN chmod +x /network_gater.sh
RUN /network_gater.sh

ENTRYPOINT  ["broadcast_network_stress_test_node"]
