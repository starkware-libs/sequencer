FROM ubuntu:24.04

# Install required runtime dependencies
RUN apt-get update \ 
    && apt-get install -y ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r committer_benchmark && useradd -m -r -g committer_benchmark committer_benchmark

# Copy the binary from local target/release directory. The binary should be built with the --release
# flag.
# Note: need to disable the .dockerignore file during the image build process if it has the "target"
# directory in it.
COPY target/release/starknet_committer_cli /usr/local/bin/starknet_committer_cli

RUN chmod +x /usr/local/bin/starknet_committer_cli
RUN chown -R committer_benchmark:committer_benchmark /usr/local/bin/starknet_committer_cli

USER committer_benchmark

# Run the storage benchmark command.
ENTRYPOINT [ "starknet_committer_cli", "storage-benchmark" ]
# Default parameters for the storage benchmark: 
# - run with 1000000 block iterations
# - generate 4000 state diffs (new leaves) per block.
# - use cached mdbx storage with a cache size of 1000000.
# - checkpoint every 1000 blocks.
# - log level is info.
# - use the provided data, storage, output and checkpoint directories (override for a persistent storage dir of
#   a specific node)

CMD ["--n-iterations", "1000000", \
    "--n-diffs", "1000", \
    "--storage-type", "cached-mdbx", \
    "--cache-size", "1000000", \
    "--checkpoint-interval", "1000", \
    "--log-level", "info", \
    "--stats-interval", "1000", \
    "--data-path", "/mnt/data/committer_storage_benchmark", \
    "--storage-path", "/mnt/data/storage", \
    "--output-dir", "/mnt/data/csvs", \
    "--checkpoint-dir", "/mnt/data/checkpoints" \
    ]
