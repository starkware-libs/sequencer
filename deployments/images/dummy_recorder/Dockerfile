#syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

# Compile the dummy_recorder crate in release mode, ensuring dependencies are locked.
FROM base AS builder
WORKDIR /app
COPY . .
RUN cargo build --bin dummy_recorder

FROM ubuntu:24.04

WORKDIR /app
COPY --from=builder /app/target/debug/dummy_recorder ./target/debug/dummy_recorder
COPY --from=builder /usr/bin/tini /usr/bin/tini

# Set permissions.
RUN chown -R ubuntu:ubuntu /app

# Expose port.
EXPOSE 8080

# Switch to ubuntu user.
USER ubuntu

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/debug/dummy_recorder"]
