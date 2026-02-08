# crates/starknet_os_runner/Dockerfile
#
# Simplified Dockerfile for the starknet_os_runner proving service.
#
# Build from repository root:
#   docker build -f crates/starknet_os_runner/Dockerfile -t os_runner:latest .
#
# Run:
#   docker run --rm -p 3000:3000 os_runner:latest --rpc-url http://pathfinder:9545 --chain-id SN_INTEGRATION_SEPOLIA

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM ubuntu:24.04 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    curl \
    git \
    libclang-dev \
    libgmp-dev \
    libssl-dev \
    libzstd-dev \
    pkg-config \
    python3 \
    python3-dev \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Install Rust.
ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH="${RUSTUP_HOME}/bin:${PATH}"
COPY rust-toolchain.toml .
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y \
    && rustup toolchain install \
    && rm rust-toolchain.toml

# Python venv with cairo-lang (needed at build time for cairo-compile).
ENV VIRTUAL_ENV=/opt/sequencer_venv
RUN python3 -m venv ${VIRTUAL_ENV}
ENV PATH="${VIRTUAL_ENV}/bin:${PATH}"
ARG CAIRO_LANG_VERSION=0.14.1a0
RUN pip install --no-cache-dir cairo-lang==${CAIRO_LANG_VERSION}

# Build starknet_os_runner.
WORKDIR /app
COPY . .
RUN cargo build --release -p starknet_os_runner

# Build stwo_run_and_prove from proving-utils.
ARG PROVING_UTILS_REV=3176b4d
RUN git clone https://github.com/starkware-libs/proving-utils /tmp/proving-utils \
    && cd /tmp/proving-utils \
    && git checkout ${PROVING_UTILS_REV} \
    && cargo build --release -p stwo_run_and_prove --bin stwo_run_and_prove \
    && cp target/release/stwo_run_and_prove /usr/local/bin/

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Binary.
COPY --from=builder /app/target/release/starknet_os_runner ./target/release/starknet_os_runner

# Resources (bootloader etc.).
COPY --from=builder /app/crates/starknet_os_runner/resources ./crates/starknet_os_runner/resources

# starknet-sierra-compile binary (needed by blockifier for Sierra->Casm compilation).
COPY --from=builder /app/target/release/shared_executables/starknet-sierra-compile ./target/release/shared_executables/starknet-sierra-compile

# stwo_run_and_prove prover binary.
COPY --from=builder /usr/local/bin/stwo_run_and_prove /usr/local/bin/stwo_run_and_prove

# Non-root user.
RUN useradd --uid 1001 --create-home --home-dir /app sequencer \
    && chown -R sequencer:sequencer /app
USER 1001

EXPOSE 3000

ENTRYPOINT ["/app/target/release/starknet_os_runner"]
