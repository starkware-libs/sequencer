# syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

# Compile the papyrus_node crate in release mode, ensuring dependencies are locked.
FROM base AS builder
COPY . .
RUN cargo build --release --package papyrus_node

FROM ubuntu:24.04

WORKDIR /app
COPY --from=builder /target/release/papyrus_node /app/target/release/papyrus_node
COPY --from=builder /usr/bin/tini /usr/bin/tini

COPY config/papyrus/ /app/config/papyrus

# Set permissions for ubuntu user
RUN chown -R ubuntu:ubuntu /app

# Expose RPC and monitoring ports.
EXPOSE 8080 8081

# Switch to ubuntu user.
USER ubuntu

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/release/papyrus_node"]
