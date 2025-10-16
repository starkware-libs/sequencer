FROM ubuntu:24.04

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from local target/release directory
COPY target/release/starknet_committer_cli /usr/local/bin/starknet_committer_cli

# Make the binary executable
RUN chmod +x /usr/local/bin/starknet_committer_cli

# Create directory for benchmark output
RUN mkdir -p /tmp/committer_storage_benchmark

# Set the working directory
WORKDIR /tmp/committer_storage_benchmark

# Run the storage benchmark command
ENTRYPOINT [ "starknet_committer_cli", "storage-benchmark" ]
CMD ["1000000", "--storage-path", "/mnt/pd-ssd-new/storage", "--data-path", "/mnt/pd-ssd-new/committer_storage_benchmark", "--checkpoint-interval", "1000"]
