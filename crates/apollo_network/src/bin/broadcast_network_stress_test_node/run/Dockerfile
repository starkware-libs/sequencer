# syntax = devthefuture/dockerfile-x
#
# Run with the project root context:
# docker build -f crates/apollo_network/src/bin/network_stress_test/cluster/Dockerfile .

INCLUDE deployments/images/base/Dockerfile

FROM base AS builder
WORKDIR /usr/src/rust_code
COPY . .
# TODO(AndrewL): use cargo chef for better caching
RUN cargo build --release --bin network_stress_test

FROM ubuntu:24.04 AS final_stage
COPY --from=builder /usr/src/rust_code/target/release/network_stress_test /usr/local/bin/network_stress_test


ENTRYPOINT  ["network_stress_test"]
