# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/

# We use Cargo Chef to compile dependencies before compiling the rest of the crates.
# This approach ensures proper Docker caching, where dependency layers are cached until a dependency changes.
# Code changes in our crates won't affect these cached layers, making the build process more efficient.
# More info on Cargo Chef: https://github.com/LukeMathWalker/cargo-chef

# We start by creating a base image using 'clux/muslrust' with additional required tools.
FROM ubuntu:22.04 AS base
WORKDIR /app

COPY scripts/install_build_tools.sh .
COPY scripts/dependencies.sh .
RUN apt update && apt -y install curl bzip2


ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

RUN ./install_build_tools.sh

RUN cargo install cargo-chef
RUN apt update && apt -y install unzip


# # Reinstalling the stable Rust toolchain to ensure a clean environment
# RUN rustup toolchain uninstall stable-x86_64-unknown-linux-gnu && rustup toolchain install stable-x86_64-unknown-linux-gnu

# # Add the x86_64-unknown-linux-musl target to rustup for compiling statically linked binaries.
# # This enables the creation of fully self-contained binaries that do not depend on the system's dynamic libraries,
# # resulting in more portable executables that can run on any Linux distribution.
# RUN rustup target add x86_64-unknown-linux-musl

#####################
# Stage 1 (planer): #
#####################
FROM base AS planner
COPY . .
# * Running Cargo Chef prepare that will generate recipe.json which will be used in the next stage.
RUN cargo chef prepare

#####################
# Stage 2 (cacher): #
#####################
# Compile all the dependecies using Cargo Chef cook.
FROM base AS cacher

# Copy recipe.json from planner stage
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --package papyrus_node

######################
# Stage 3 (builder): #
######################
FROM base AS builder
COPY . .
COPY --from=cacher /app/target target
# Disable incremental compilation for a cleaner build.
ENV CARGO_INCREMENTAL=0

# Compile the papyrus_node crate for the x86_64-unknown-linux-musl target in release mode, ensuring dependencies are locked.
RUN cargo build --release --package papyrus_node --locked

###########################
# Stage 4 (papyrus_node): #
###########################
FROM base AS papyrus_node
ENV ID=1000
WORKDIR /app

# Copy the node executable and its configuration.
COPY --from=builder /app/target/release/papyrus_node /app/target/release/papyrus_node
COPY config config

# Install tini, a lightweight init system, to call our executable.
RUN apt install tini

# Create a new user "papyrus".
RUN set -ex; \
    addgroup --gid ${ID} papyrus; \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home /app papyrus; \
    chown -R papyrus:papyrus /app

# Expose RPC and monitoring ports.
EXPOSE 8080 8081

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/release/papyrus_node"]
