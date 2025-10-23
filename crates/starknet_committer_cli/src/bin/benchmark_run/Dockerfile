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
# - use the provided storage directory and data directory (override for a persistent storage dir of
#   a specific node)
# - checkpoint every 1000 blocks.
# - generate 4000 state diffs (new leaves) per block.
CMD ["1000000", \
    "--storage-path", "/mnt/pd-ssd-new/storage", \
    "--data-path", "/mnt/pd-ssd-new/committer_storage_benchmark", \
    "--checkpoint-interval", "1000" \
    "--n-diffs", "4000" \
    ]
