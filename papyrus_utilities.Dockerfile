# syntax = devthefuture/dockerfile-x

# The first line and the "INCLUDE Dockerfile" enable us to use the builder stage from the main Dockerfile.
# The DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 in the image creation command is to be able to use the dockerfile-x syntax.

# To build the papyrus utilities image, run from the root of the project:
# DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 docker build -f papyrus_utilities.Dockerfile .

INCLUDE deployments/images/base/Dockerfile

# Build papyrus utilities.
FROM base AS builder
COPY . .

# Build storage_benchmark.
RUN cargo build --release --package apollo_storage \
    --features "clap statistical" --bin storage_benchmark

# Starting a new stage so that the final image will contain only the executables.
FROM ubuntu:22.04

# Copy the storage_benchmark executable.
COPY --from=builder /target/release/storage_benchmark /target/release/storage_benchmark

# Set the PATH environment variable to enable running an executable only with its name.
ENV PATH="/target/release:${PATH}"

ENTRYPOINT echo -e \
    "There is no default executable for this image. Run an executable using its name or path to it.\n\
    The available executables are:\n\
    - storage_benchmark, performs a benchmark on the storage.\n\
    For example, in a docker runtime: docker run --entrypoint storage_benchmark <image>"
