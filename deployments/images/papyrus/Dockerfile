# syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile


# Compile the papyrus_node crate in release mode, ensuring dependencies are locked.
FROM base AS builder
COPY . .
RUN rustup toolchain install
RUN cargo build --release --package papyrus_node

FROM ubuntu:24.04

ENV ID=1001
WORKDIR /app
COPY --from=builder /target/release/papyrus_node /app/target/release/papyrus_node
COPY --from=builder /usr/bin/tini /usr/bin/tini

COPY config/papyrus config/papyrus

# Create a new user "papyrus".
RUN set -ex; \
    groupadd --gid ${ID} papyrus; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app papyrus; \
    chown -R papyrus:papyrus /app

# Expose RPC and monitoring ports.
EXPOSE 8080 8081

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/release/papyrus_node"]
