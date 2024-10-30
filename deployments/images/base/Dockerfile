# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/

# We use Cargo Chef to compile dependencies before compiling the rest of the crates.
# This approach ensures proper Docker caching, where dependency layers are cached until a dependency changes.
# Code changes in our crates won't affect these cached layers, making the build process more efficient.
# More info on Cargo Chef: https://github.com/LukeMathWalker/cargo-chef

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

#####################
# Stage 1 (planer): #
#####################
FROM base AS planner
COPY . .
# * Running Cargo Chef prepare that will generate recipe.json which will be used in the next stage.
RUN cargo chef prepare
