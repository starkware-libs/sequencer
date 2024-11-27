# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/

# We use Cargo Chef to compile dependencies before compiling the rest of the crates.
# This approach ensures proper Docker caching, where dependency layers are cached until a dependency changes.
# Code changes in our crates won't affect these cached layers, making the build process more efficient.
# More info on Cargo Chef: https://github.com/LukeMathWalker/cargo-chef

FROM ubuntu:22.04 AS base
# WORKDIR /app

COPY scripts/install_build_tools.sh .
COPY scripts/dependencies.sh .
RUN apt update && apt -y install \
    bzip2 \
    curl \
    tini \
    unzip

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

RUN ./install_build_tools.sh

# Cleanup.
RUN rm -f install_build_tools.sh dependencies.sh
