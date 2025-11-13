# syntax=docker/dockerfile:1.6

# ---- Stage 1: Builder (has cargo/rustc)
FROM rust:1.89 AS builder
WORKDIR /app

# System deps (compile-time)
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config cmake clang lld \
 && rm -rf /var/lib/apt/lists/*

# Copy only manifests first so deps cache well
COPY Cargo.toml Cargo.lock ./
# If this is a workspace, copy members' manifests (adjust the glob if needed)
COPY crates/*/Cargo.toml crates/*/Cargo.toml

# Minimal stub so cargo can resolve deps
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Pre-build deps (cached)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release -p starknet_committer_cli --bin starknet_committer_cli || true

# Bring real sources
COPY . .

# Real build (cached), then copy artifact OUT of the cache mount into a committed layer
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release -p starknet_committer_cli --bin starknet_committer_cli \
 && mkdir -p /out \
 && cp /app/target/release/starknet_committer_cli /out/

# ---- Stage 2: Runtime
FROM ubuntu:24.04 AS runtime-base

# Minimal runtime deps
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd -r committer_benchmark && useradd -m -r -g committer_benchmark committer_benchmark

# Copy binary from builder with perms set in one shot
COPY --from=builder --chmod=0755 --chown=committer_benchmark:committer_benchmark \
  /out/starknet_committer_cli /usr/local/bin/starknet_committer_cli

USER committer_benchmark

# Default ENTRYPOINT
ENTRYPOINT ["starknet_committer_cli", "storage-benchmark"]

# Default args (can be overridden at run-time)
CMD ["--n-iterations", "1000000", \
     "--flavor", "overlap-1k-diff", \
     "--storage-type", "cached-rocksdb", \
     "--cache-size", "10000000", \
     "--checkpoint-interval", "1000", \
     "--log-level", "info", \
     "--data-path", "/mnt/data/committer_storage_benchmark", \
     "--storage-path", "/mnt/data/storage", \
     "--output-dir", "/mnt/data/csvs", \
     "--checkpoint-dir", "/mnt/data/checkpoints"]
