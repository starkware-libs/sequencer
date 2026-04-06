# crates/blockifier_reexecution/replay/Dockerfile
#
# Self-contained Dockerfile for building the blockifier_reexecution rpc-replay tool
# with Cairo Native support.
#
# Build:
#   docker build -f crates/blockifier_reexecution/replay/Dockerfile -t blockifier-reexecution .
#
# Run:
#   docker run --rm blockifier-reexecution rpc-replay -n <RPC_URL> -c testnet --start-block 800000

# =============================================================================
# Stage 1: Base image with Rust toolchain and LLVM 19 (for Cairo Native)
# =============================================================================
FROM ubuntu:24.04 AS base

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
    lsb-release \
    pkg-config \
    python3 \
    python3-dev \
    python3-pip \
    python3-venv \
    software-properties-common \
    tini \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Install LLVM 19 (required for Cairo Native compilation).
RUN curl https://apt.llvm.org/llvm.sh -Lo llvm.sh \
    && bash ./llvm.sh 19 all \
    && rm -f ./llvm.sh \
    && apt-get update && apt-get install -y --no-install-recommends \
        libgmp3-dev \
        libmlir-19-dev \
        libpolly-19-dev \
        mlir-19-tools \
    && rm -rf /var/lib/apt/lists/*

ENV LLVM_SYS_191_PREFIX="/usr/lib/llvm-19/"
ENV MLIR_SYS_190_PREFIX="/usr/lib/llvm-19/"
ENV TABLEGEN_190_PREFIX="/usr/lib/llvm-19/"

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH="${RUSTUP_HOME}/bin:${PATH}"

COPY rust-toolchain.toml rust-toolchain.toml
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none \
    && rustup toolchain install \
    && cargo install cargo-chef \
    && rm rust-toolchain.toml

# =============================================================================
# Stage 2: Planner - prepare cargo-chef recipe
# =============================================================================
FROM base AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 3: Builder - compile the binary
# =============================================================================
FROM base AS builder
WORKDIR /app

COPY rust-toolchain.toml rust-toolchain.toml
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release -p blockifier_reexecution --features cairo_native --recipe-path recipe.json

COPY . .
RUN cargo build --release -p blockifier_reexecution --features cairo_native

# Install compiler binaries used for Sierra compilation at runtime.
RUN scripts/install_compiler_binaries.sh && \
    cp "$(which starknet-sierra-compile)" /usr/local/bin/ && \
    cp "$(which starknet-native-compile)" /usr/local/bin/

# =============================================================================
# Stage 4: Final runtime image
# =============================================================================
FROM ubuntu:24.04 AS final_stage

# ca-certificates: HTTPS requests
# binutils, libc6-dev: required by starknet-native-compile at runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    binutils \
    libc6-dev \
    && rm -rf /var/lib/apt/lists/*

ENV ID=1001
WORKDIR /app

# Copy the binary and both compilers (Sierra-to-CASM and Sierra-to-Native).
COPY --from=builder /app/target/release/blockifier_reexecution ./target/release/blockifier_reexecution
COPY --from=builder /usr/local/bin/starknet-sierra-compile /usr/local/bin/starknet-sierra-compile
COPY --from=builder /usr/local/bin/starknet-native-compile /usr/local/bin/starknet-native-compile
COPY --from=builder /usr/bin/tini /usr/bin/tini

RUN set -ex; \
    groupadd --gid ${ID} reexecution; \
    useradd --gid ${ID} --uid ${ID} --no-create-home reexecution

USER reexecution

ENTRYPOINT ["sh", "-c", "exec tini -- /app/target/release/blockifier_reexecution \"$@\"", "--"]
