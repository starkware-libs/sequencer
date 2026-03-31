# crates/starknet_transaction_prover/Dockerfile
#
# Self-contained Dockerfile for building the starknet_transaction_prover service.
# This Dockerfile is designed for external parties to build and run the tx prover
# without requiring the full internal build infrastructure.
#
# Build:
#   docker build -f crates/starknet_transaction_prover/Dockerfile -t tx_prover:latest .
#
# Run:
#   docker run --rm -p 3000:3000 tx_prover:latest
#
# Key differences from the internal Dockerfile (deployments/images/sequencer/Dockerfile):
#   - No dockerfile-x INCLUDE directive
#   - No PyPy 3.9
#   - No LLVM 19 dependency (cairo_native feature disabled)
#   - Minimal runtime dependencies

# =============================================================================
# Stage 1: Base image with Rust toolchain
# =============================================================================
FROM ubuntu:24.04 AS base

# Install build dependencies.
# - build-essential, pkg-config: native compilation
# - clang, libclang-dev: for bindgen (mdbx-sys, zstd-sys)
# - cmake: native dependency builds
# - python3, python3-dev, python3-pip, python3-venv: for cairo-lang compiler
# - libgmp-dev: required by cairo-lang
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
    tini \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain.
ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH="${RUSTUP_HOME}/bin:${PATH}"

# Use the crate-level rust-toolchain.toml (nightly).
COPY crates/starknet_transaction_prover/rust-toolchain.toml rust-toolchain.toml

# --default-toolchain none: skip installing the default stable toolchain, since we only need
# the nightly defined in rust-toolchain.toml.
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none \
    && rustup toolchain install \
    && cargo install cargo-chef \
    && rm rust-toolchain.toml

# Create Python venv and install cairo-lang (required for compiling Starknet OS).
ENV VIRTUAL_ENV=/opt/sequencer_venv
RUN python3 -m venv ${VIRTUAL_ENV}
ENV PATH="${VIRTUAL_ENV}/bin:${PATH}"

# Install cairo-lang package for cairo-compile tool.
# This is the minimum required for building starknet_transaction_prover.
ARG CAIRO_LANG_VERSION=0.14.1a0
RUN pip install --no-cache-dir cairo-lang==${CAIRO_LANG_VERSION}

# =============================================================================
# Stage 2: Planner - prepare cargo-chef recipe
# =============================================================================
FROM base AS planner
WORKDIR /app
COPY . .
COPY crates/starknet_transaction_prover/rust-toolchain.toml rust-toolchain.toml
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 3: Builder - compile the binary
# =============================================================================
FROM base AS builder
WORKDIR /app

ARG BUILD_MODE=release
ARG TARGET_CPU=""
ENV BUILD_MODE=${BUILD_MODE}
ENV TARGET_CPU=${TARGET_CPU}

# Validate BUILD_MODE value.
RUN if [ "$BUILD_MODE" != "release" ] && [ "$BUILD_MODE" != "debug" ]; then \
    echo "Error: BUILD_MODE must be either 'release' or 'debug' (got '$BUILD_MODE')" >&2; \
    exit 1; \
    fi

# Cache dependencies with cargo-chef.
# Use -p starknet_transaction_prover to avoid building LLVM-dependent crates (e.g., cairo_native).
COPY --from=planner /app/recipe.json recipe.json
COPY crates/starknet_transaction_prover/rust-toolchain.toml rust-toolchain.toml
# TARGET_CPU is injected through profile env vars so cargo-chef and its child cargo builds use
# the same optimized profile, while build scripts and proc-macros stay on generic host settings.
RUN BUILD_FLAGS=$([ "$BUILD_MODE" = "release" ] && echo "--release" || true); \
    if [ -n "$TARGET_CPU" ]; then \
        PROFILE_ENV_PREFIX=$([ "$BUILD_MODE" = "release" ] && echo "CARGO_PROFILE_RELEASE" || echo "CARGO_PROFILE_DEV"); \
        env \
            CARGO_UNSTABLE_PROFILE_RUSTFLAGS=true \
            "${PROFILE_ENV_PREFIX}_RUSTFLAGS=-C target-cpu=${TARGET_CPU}" \
            "${PROFILE_ENV_PREFIX}_BUILD_OVERRIDE_RUSTFLAGS=" \
            cargo chef cook $BUILD_FLAGS \
                -p starknet_transaction_prover \
                --features stwo_proving \
                --recipe-path recipe.json; \
    else \
        cargo chef cook $BUILD_FLAGS -p starknet_transaction_prover --features stwo_proving --recipe-path recipe.json; \
    fi

# Copy source and build starknet_transaction_prover (without cairo_native feature).
COPY . .
COPY crates/starknet_transaction_prover/rust-toolchain.toml rust-toolchain.toml
RUN BUILD_FLAGS=$([ "$BUILD_MODE" = "release" ] && echo "--release" || true); \
    if [ -n "$TARGET_CPU" ]; then \
        PROFILE_ENV_PREFIX=$([ "$BUILD_MODE" = "release" ] && echo "CARGO_PROFILE_RELEASE" || echo "CARGO_PROFILE_DEV"); \
        env \
            CARGO_UNSTABLE_PROFILE_RUSTFLAGS=true \
            "${PROFILE_ENV_PREFIX}_RUSTFLAGS=-C target-cpu=${TARGET_CPU}" \
            "${PROFILE_ENV_PREFIX}_BUILD_OVERRIDE_RUSTFLAGS=" \
            cargo build $BUILD_FLAGS \
                -p starknet_transaction_prover \
                --features stwo_proving; \
    else \
        cargo build $BUILD_FLAGS -p starknet_transaction_prover --features stwo_proving; \
    fi

# Install compiler binary used for Sierra to CASM compilation at runtime.
# Version is read from a plain text file (the single source of truth).
RUN cargo install --locked starknet-sierra-compile --version "$(cat crates/apollo_infra_utils/src/cairo_compiler_version.txt)"

# =============================================================================
# Stage 4: Final runtime image
# =============================================================================
FROM ubuntu:24.04 AS final_stage

ARG BUILD_MODE=release
ENV BUILD_MODE=${BUILD_MODE}

# Install only runtime dependencies.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ENV ID=1001
WORKDIR /app

# Copy the binary.
COPY --from=builder /app/target/${BUILD_MODE}/starknet_transaction_prover ./target/${BUILD_MODE}/starknet_transaction_prover

# Copy resources needed by the service (includes bootloader).
COPY --from=builder /app/crates/starknet_transaction_prover/resources ./crates/starknet_transaction_prover/resources

# Copy starknet-sierra-compile binary required for Sierra to Casm compilation.
COPY --from=builder /var/tmp/rust/bin/starknet-sierra-compile /usr/local/bin/starknet-sierra-compile

# Copy tini for proper init handling.
COPY --from=builder /usr/bin/tini /usr/bin/tini

# Create a non-root user.
RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer; \
    mkdir -p /data; \
    chown -R sequencer:sequencer /app /data

# OCI metadata labels.
# source: links the GHCR package to the repository (inherits visibility settings).
# description: short summary shown on the package page.
# documentation: links to the crate README.
LABEL org.opencontainers.image.source="https://github.com/starkware-libs/sequencer"
LABEL org.opencontainers.image.description="A standalone JSON-RPC service that generates STARK proofs for individual Starknet transactions"
LABEL org.opencontainers.image.documentation="https://github.com/starkware-libs/sequencer/blob/main/crates/starknet_transaction_prover/README.md"

# Expose the JSON-RPC server port.
EXPOSE 3000

# Switch to non-root user.
USER ${ID}

# Use tini as the init process.
ENTRYPOINT ["sh", "-c", "exec tini -- /app/target/$BUILD_MODE/starknet_transaction_prover \"$@\"", "--"]
