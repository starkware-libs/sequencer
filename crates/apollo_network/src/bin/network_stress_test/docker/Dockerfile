# syntax = devthefuture/dockerfile-x
# File to run the network_stress_test in a container
#
# Command to build the container:
# docker build -f crates/apollo_network/src/bin/network_stress_test/Dockerfile .
#
# Run command from repository root 
# 
# For verbose output:
# docker build --progress=plain --no-cache -f crates/apollo_network/src/bin/network_stress_test/Dockerfile .

INCLUDE deployments/images/base/Dockerfile

FROM base AS builder
WORKDIR /usr/src/rust_code
COPY . .
RUN cargo build --release --bin network_stress_test

# Final Stage: Runtime stage to create the final, smaller image
FROM ubuntu:24.04 AS final_stage
# Update package lists and install necessary runtime dependencies
# --no-install-recommends avoids installing unnecessary suggested packages
# RUN apt-get update
# RUN apt-get install -y --no-install-recommends libssl-dev ca-certificates gcc libstdc++6
# # RUN apt-get install -y software-properties-common
# # RUN add-apt-repository -y ppa:ubuntu-toolchain-r/test
# # RUN apt update
# RUN apt-get install 

# RUN apt-get update
# RUN apt-get install -y glibc-source

# RUN rm -rf /var/lib/apt/lists/*
# Copy the specific compiled example binary from the builder stage
# The binary will be located in the target/release/examples directory
COPY --from=builder /usr/src/rust_code/target/release/network_stress_test /usr/local/bin/network_stress_test

# Set the default command to run the specified example executable
ENTRYPOINT  ["network_stress_test"]