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

# Validate BUILD_MODE value.
RUN if [ "$BUILD_MODE" != "release" ] && [ "$BUILD_MODE" != "debug" ] && [ "$BUILD_MODE" != "profiling" ]; then \
        echo "Error: BUILD_MODE must be 'release', 'debug', or 'profiling' (got '$BUILD_MODE')" >&2; \
        exit 1; \
    fi

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .

RUN BUILD_FLAGS=""; \
    if [ "$BUILD_MODE" = "release" ]; then \
        BUILD_FLAGS="--release"; \
    elif [ "$BUILD_MODE" = "profiling" ]; then \
        BUILD_FLAGS="--profile profiling"; \
    fi; \
    cargo build $BUILD_FLAGS --bin apollo_node --features cairo_native

# Install compiler binaries used for Sierra compilation at runtime.
# Binaries land under $CARGO_TOOLS_ROOT/<binary>-<version>/bin/<binary>; the
# final stage below COPYs the entire $CARGO_TOOLS_ROOT tree so this layout is
# preserved at runtime.
RUN scripts/install_compiler_binaries.sh


FROM ubuntu:24.04 AS final_stage

ARG BUILD_MODE=release
ENV BUILD_MODE=${BUILD_MODE}

# Required for https requests: ca-certificates
# Required for starknet-native-compile: binutils libc6-dev
RUN apt-get update && apt-get install -y ca-certificates binutils libc6-dev

# Install heaptrack for on-demand memory profiling (profiling builds only).
RUN if [ "$BUILD_MODE" = "profiling" ]; then \
        apt-get install -y heaptrack; \
    fi

ENV ID=1001
WORKDIR /app
COPY --from=builder /app/target/${BUILD_MODE}/apollo_node ./target/${BUILD_MODE}/apollo_node
# Per-version compiler binaries; their absolute path is constructed from
# $CARGO_TOOLS_ROOT at runtime.
ENV CARGO_TOOLS_ROOT=/opt/cargo-tools
COPY --from=builder /var/tmp/rust/tools ${CARGO_TOOLS_ROOT}
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
#
# Profiling usage: the node starts normally. To attach heaptrack on demand:
#   1. Deploy with SYS_PTRACE capability (securityContext.capabilities.add: ["SYS_PTRACE"]).
#   2. kubectl exec -it <pod> -- sh -c 'heaptrack --pid $(pidof apollo_node) -o /data/heaptrack'
#   3. Stop with Ctrl+C, then: kubectl cp <pod>:/data/heaptrack.*.zst ./heaptrack_output.zst
#   4. Analyze: heaptrack_print ./heaptrack_output.zst
ENTRYPOINT ["sh", "-c", "exec tini -- /app/target/$BUILD_MODE/apollo_node \"$@\"", "--"]
