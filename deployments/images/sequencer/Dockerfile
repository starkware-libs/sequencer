# syntax = devthefuture/dockerfile-x
# deployments/images/sequencer/Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/
# We use dockerfile-x, for more information visit: https://github.com/devthefuture-org/dockerfile-x/blob/master/README.md

INCLUDE deployments/images/base/Dockerfile

FROM base AS planner
WORKDIR /app
COPY . .
# Installing rust version in rust-toolchain.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /app

ARG BUILD_MODE=release
ENV BUILD_MODE=${BUILD_MODE}

# Validate BUILD_MODE value
RUN if [ "$BUILD_MODE" != "release" ] && [ "$BUILD_MODE" != "debug" ]; then \
        echo "Error: BUILD_MODE must be either 'release' or 'debug' (got '$BUILD_MODE')" >&2; \
        exit 1; \
    fi


COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .

RUN BUILD_FLAGS=""; \
    if [ "$BUILD_MODE" = "release" ]; then \
        BUILD_FLAGS="--release"; \
    fi; \
    cargo build $BUILD_FLAGS --bin apollo_node --features cairo_native


FROM ubuntu:24.04 AS final_stage

ARG BUILD_MODE=release
ENV BUILD_MODE=${BUILD_MODE}

# Required for https requests
RUN apt-get update && apt-get install -y ca-certificates

ENV ID=1001
WORKDIR /app
COPY --from=builder /app/target/${BUILD_MODE}/apollo_node ./target/${BUILD_MODE}/apollo_node
COPY --from=builder /app/target/${BUILD_MODE}/shared_executables/starknet-sierra-compile ./target/${BUILD_MODE}/shared_executables/starknet-sierra-compile
COPY --from=builder /app/target/${BUILD_MODE}/shared_executables/starknet-native-compile ./target/${BUILD_MODE}/shared_executables/starknet-native-compile
COPY --from=builder /usr/bin/tini /usr/bin/tini

# Copy apollo config schema, used when it loads its configuration.
COPY crates/apollo_node/resources/config_schema.json crates/apollo_node/resources/config_schema.json

# Create a new user "sequencer".
RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer; \
    mkdir /data; \
    chown -R sequencer:sequencer /app /data

# Expose RPC and monitoring ports.
EXPOSE 8080 8081 8082

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process, while evaluating the build mode, and passing any arguments.
ENTRYPOINT ["sh", "-c", "exec tini -- /app/target/$BUILD_MODE/apollo_node \"$@\"", "--"]
