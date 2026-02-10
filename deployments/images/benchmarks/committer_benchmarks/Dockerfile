# syntax = devthefuture/dockerfile-x
INCLUDE deployments/images/base/Dockerfile

# ---- Stage 1: Builder (has cargo/rustc)
FROM base AS builder
WORKDIR /app

COPY . .

# Build (cache mounts speed up rebuilds), then copy artifact out of the cache mount
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

# Run the storage benchmark command.
ENTRYPOINT [ "starknet_committer_cli", "storage-benchmark" ]

CMD ["cached-rocksdb", \
    "--n-iterations", "6000000", \
    "--flavor", "mainnet", \
    "--n-updates", "1000", \
    "--cache-size", "40000000", \
    "--checkpoint-interval", "1000", \
    "--include-inner-stats", \
    "--log-level", "info", \
    "--data-path", "/mnt/data/committer_storage_benchmark", \
    "--storage-path", "/mnt/data/storage", \
    "--output-dir", "/mnt/data/csvs", \
    "--checkpoint-dir", "/mnt/data/checkpoints" \
    ]
