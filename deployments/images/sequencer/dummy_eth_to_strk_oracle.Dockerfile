# syntax = devthefuture/dockerfile-x
# deployments/images/sequencer/dummy_eth_to_strk_oracle.Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/
# We use dockerfile-x, for more information visit: https://github.com/devthefuture-org/dockerfile-x/blob/master/README.md

INCLUDE deployments/images/base/Dockerfile

FROM base AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json
COPY . .
RUN SKIP_NATIVE_COMPILE_VALIDATION=1 SKIP_SIERRA_COMPILE_VALIDATION=1 cargo build

FROM ubuntu:24.04 AS final_stage

ENV ID=1001
WORKDIR /app
COPY --from=builder /app/target/debug/dummy_eth_to_strk_oracle ./target/debug/dummy_eth_to_strk_oracle
COPY --from=builder /usr/bin/tini /usr/bin/tini

RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer;

EXPOSE 9000

USER ${ID}

ENTRYPOINT ["tini", "--", "/app/target/debug/dummy_eth_to_strk_oracle"]
