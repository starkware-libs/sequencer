# syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --package papyrus_node

######################
# Stage 3 (builder): #
######################
FROM base AS builder
COPY . .
COPY --from=cacher /app/target target
# Disable incremental compilation for a cleaner build.
ENV CARGO_INCREMENTAL=0

# Compile the papyrus_node crate in release mode, ensuring dependencies are locked.
RUN cargo build --release --package papyrus_node

###########################
# Stage 4 (papyrus_node): #
###########################
FROM base AS papyrus_node
ENV ID=1000
WORKDIR /app

# Copy the node executable and its configuration.
COPY --from=builder /app/target/release/papyrus_node /app/target/release/papyrus_node
COPY config config


# Create a new user "papyrus".
RUN set -ex; \
    addgroup --gid ${ID} papyrus; \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home /app papyrus; \
    chown -R papyrus:papyrus /app

# Expose RPC and monitoring ports.
EXPOSE 8080 8081

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["tini", "--", "/app/target/release/papyrus_node"]
