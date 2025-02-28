#syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

FROM base AS builder
WORKDIR /app
COPY . .
RUN rustup toolchain install
RUN cargo build --bin dummy_recorder

FROM ubuntu:24.04

ENV ID=1001
WORKDIR /app
COPY --from=builder /app/target/debug/dummy_recorder ./target/debug/dummy_recorder
COPY --from=builder /usr/bin/tini /usr/bin/tini

RUN set -ex; \
    groupadd --gid ${ID} sequencer; \
    useradd --gid ${ID} --uid ${ID} --comment "" --create-home --home-dir /app sequencer;

EXPOSE 8080

USER ${ID}

ENTRYPOINT ["tini", "--", "/app/target/debug/dummy_recorder"]
