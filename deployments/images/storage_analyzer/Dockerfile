# deployments/images/storage_analyzer/Dockerfile
#
# Standalone Dockerfile for the storage analyzer binary.
# Build: docker build -f deployments/images/storage_analyzer/Dockerfile -t storage-analyzer .
# Run:   docker run -v /path/to/data:/data storage-analyzer --db-path /data/SN_MAIN --scope state_only --json

FROM rust:1.94 AS builder

RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    libclang-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release -p apollo_storage --features storage_analyzer --bin storage_analyzer

FROM ubuntu:24.04

RUN apt-get update && apt-get install -y ca-certificates tini && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/storage_analyzer /usr/local/bin/storage_analyzer

ENV ID=1001
RUN groupadd --gid ${ID} analyzer && \
    useradd --gid ${ID} --uid ${ID} --create-home analyzer && \
    mkdir /data && chown analyzer:analyzer /data

USER ${ID}

ENTRYPOINT ["tini", "--", "storage_analyzer"]
