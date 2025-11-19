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

CMD ["--preset", "cached-rocksdb"]
