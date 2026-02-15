# crates/starknet_os_runner/Dockerfile
#
# Self-contained Dockerfile for building the starknet_os_runner service.
# This Dockerfile is designed for external parties to build and run the OS runner
# without requiring the full internal build infrastructure.
#
# Build using the wrapper script (recommended):
#   ./scripts/build_starknet_os_runner.sh
#
# Or build manually:
#   ./scripts/install_stwo_run_and_prove.sh
#   docker build -f crates/starknet_os_runner/Dockerfile \
#     --build-arg PROVING_UTILS_DIR=build/proving-utils \
#     -t os_runner:latest .
#
# Run:
#   docker run --rm -p 3000:3000 os_runner:latest
#
# Key differences from the internal Dockerfile (deployments/images/sequencer/Dockerfile):
#   - No dockerfile-x INCLUDE directive
#   - No PyPy 3.9
#   - No LLVM 19 dependency (cairo_native feature disabled)
#   - Minimal runtime dependencies

# Global ARG — declared once with the default value.
ARG PROVING_UTILS_DIR=build/proving-utils

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

# Copy rust-toolchain.toml to get the correct Rust version.
COPY rust-toolchain.toml .

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y \
    && rustup toolchain install \
    && cargo install cargo-chef \
    && rm rust-toolchain.toml

# Create Python venv and install cairo-lang (required for compiling Starknet OS).
ENV VIRTUAL_ENV=/opt/sequencer_venv
RUN python3 -m venv ${VIRTUAL_ENV}
ENV PATH="${VIRTUAL_ENV}/bin:${PATH}"

# Install cairo-lang package for cairo-compile tool.
# This is the minimum required for building starknet_os_runner.
ARG CAIRO_LANG_VERSION=0.14.1a0
RUN pip install --no-cache-dir cairo-lang==${CAIRO_LANG_VERSION}

# =============================================================================
# Stage 2: Starknet OS Runner Planner - prepare cargo-chef recipe
# =============================================================================
FROM base AS starknet-os-runner-planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 3: Proving Utils Planner - prepare cargo-chef recipe
# =============================================================================
FROM base AS proving-utils-planner
WORKDIR /app
ARG PROVING_UTILS_DIR
COPY ${PROVING_UTILS_DIR} .
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 4: Starknet OS Runner Builder - compile the binary
# =============================================================================
FROM base AS starknet-os-runner-builder
WORKDIR /app

ARG BUILD_MODE=release
ENV BUILD_MODE=${BUILD_MODE}

# Validate BUILD_MODE value.
RUN if [ "$BUILD_MODE" != "release" ] && [ "$BUILD_MODE" != "debug" ]; then \
    echo "Error: BUILD_MODE must be either 'release' or 'debug' (got '$BUILD_MODE')" >&2; \
    exit 1; \
    fi

# Cache dependencies with cargo-chef.
# Use -p starknet_os_runner to avoid building LLVM-dependent crates (e.g., cairo_native).
COPY --from=starknet-os-runner-planner /app/recipe.json recipe.json
RUN BUILD_FLAGS=$([ "$BUILD_MODE" = "release" ] && echo "--release" || true); \
    cargo chef cook $BUILD_FLAGS -p starknet_os_runner --recipe-path recipe.json

# Copy source and build starknet_os_runner (without cairo_native feature).
COPY . .
RUN BUILD_FLAGS=$([ "$BUILD_MODE" = "release" ] && echo "--release" || true); \
    cargo build $BUILD_FLAGS -p starknet_os_runner

# =============================================================================
# Stage 5: Proving Utils Builder - compile the binary
# =============================================================================
# Proving-utils (and its dependency stwo) require a specific nightly Rust for
# #![feature(...)]. We copy rust-toolchain.toml from the planner so that cargo
# uses the pinned nightly version before the full source is copied.
FROM base AS proving-utils-builder
WORKDIR /app
ARG PROVING_UTILS_DIR
COPY --from=proving-utils-planner /app/rust-toolchain.toml rust-toolchain.toml
RUN rustup toolchain install
COPY --from=proving-utils-planner /app/recipe.json recipe.json
RUN cargo chef cook --release -p stwo-run-and-prove --recipe-path recipe.json

COPY ${PROVING_UTILS_DIR} .
RUN cargo build --release -p stwo-run-and-prove --bin stwo-run-and-prove \
    && mkdir -p /app/target/tools \
    && cp target/release/stwo-run-and-prove /app/target/tools/stwo_run_and_prove \
    && cp crates/cairo-program-runner-lib/resources/compiled_programs/bootloaders/simple_bootloader_compiled.json \
       /app/target/tools/simple_bootloader_compiled.json

# =============================================================================
# Stage 6: Final runtime image
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
COPY --from=starknet-os-runner-builder /app/target/${BUILD_MODE}/starknet_os_runner ./target/${BUILD_MODE}/starknet_os_runner

# Copy resources needed by the service.
COPY --from=starknet-os-runner-builder /app/crates/starknet_os_runner/resources ./crates/starknet_os_runner/resources

# Copy bootloader from proving-utils (overwrites or adds to resources).
COPY --from=proving-utils-builder /app/target/tools/simple_bootloader_compiled.json ./crates/starknet_os_runner/resources/simple_bootloader_compiled.json

# Copy stwo_run_and_prove binary (resolved via target/tools/ by the proving_utils crate).
COPY --from=proving-utils-builder /app/target/tools/stwo_run_and_prove ./target/tools/stwo_run_and_prove

# Copy starknet-sierra-compile binary required for Sierra to Casm compilation.
COPY --from=starknet-os-runner-builder /app/target/${BUILD_MODE}/shared_executables/starknet-sierra-compile ./target/${BUILD_MODE}/shared_executables/starknet-sierra-compile

# Copy tini for proper init handling.
COPY --from=starknet-os-runner-builder /usr/bin/tini /usr/bin/tini

# Create a non-root user.
RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer; \
    mkdir -p /data; \
    chown -R sequencer:sequencer /app /data

# Expose the JSON-RPC server port.
EXPOSE 3000

# Switch to non-root user.
USER ${ID}

# Use tini as the init process.
ENTRYPOINT ["sh", "-c", "exec tini -- /app/target/$BUILD_MODE/starknet_os_runner \"$@\"", "--"]
