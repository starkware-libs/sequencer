# deployments/images/base/Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/
# We use dockerfile-x, for more information visit: https://github.com/devthefuture-org/dockerfile-x/blob/master/README.md

FROM ubuntu:24.04 AS base

COPY scripts/install_build_tools.sh .
COPY scripts/dependencies.sh .
COPY rust-toolchain.toml .

RUN apt update && apt -y install \
    bzip2 \
    curl \
    tini \
    unzip

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

RUN ./install_build_tools.sh
RUN rustup toolchain install
RUN cargo install cargo-chef

# Cleanup.
RUN rm -f install_build_tools.sh dependencies.sh
