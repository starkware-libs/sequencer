# deployments/images/base/Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/
# We use dockerfile-x, for more information visit: https://github.com/devthefuture-org/dockerfile-x/blob/master/README.md

FROM ubuntu:24.04 AS base

COPY scripts/install_build_tools.sh scripts/install_build_tools.sh
COPY scripts/install_cargo_tools.sh scripts/install_cargo_tools.sh
COPY scripts/dependencies.sh scripts/dependencies.sh
COPY scripts/apt_utils.sh scripts/apt_utils.sh
COPY scripts/cargo_tool_utils.sh scripts/cargo_tool_utils.sh
COPY scripts/requirements.txt scripts/requirements.txt
COPY rust-toolchain.toml .

# If Ubuntu registry is down or slow (30s timeout), use Azure mirror instead.
RUN (timeout 30s apt-get update \
    || (sed -i 's%http://\(archive\|security\).ubuntu.com/%http://azure.archive.ubuntu.com/%g' /etc/apt/sources.list.d/ubuntu.sources \
        && apt-get update)) \
    && apt -y install \
    bzip2 \
    curl \
    tini \
    unzip

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

RUN scripts/install_build_tools.sh
RUN rustup toolchain install
RUN cargo install --locked cargo-chef

ENV LLVM_SYS_191_PREFIX="/usr/lib/llvm-19/"
ENV MLIR_SYS_190_PREFIX="/usr/lib/llvm-19/"
ENV TABLEGEN_190_PREFIX="/usr/lib/llvm-19/"

# Define python venv and install python packages.
ENV VIRTUAL_ENV=/opt/sequencer_venv
RUN python3 -m venv ${VIRTUAL_ENV}
RUN ${VIRTUAL_ENV}/bin/pip install -r scripts/requirements.txt
ENV PATH="${VIRTUAL_ENV}/bin:$PATH"

# Cleanup.
RUN rm -f scripts/install_build_tools.sh scripts/install_cargo_tools.sh scripts/cargo_tool_utils.sh scripts/dependencies.sh scripts/apt_utils.sh scripts/requirements.txt
