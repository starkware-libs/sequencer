#syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

FROM base AS builder
WORKDIR /app
COPY . .
RUN cargo build --bin dummy_recorder

FROM ubuntu:24.04

ENV ID=1000
WORKDIR /app
COPY --from=builder /app/target/debug/dummy_recorder ./target/debug/dummy_recorder
COPY --from=builder /usr/bin/tini /usr/bin/tini

RUN set -ex; \
    addgroup --gid ${ID} dummy_recorder; \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home /app dummy_recorder; \
    chown -R dummy_recorder:dummy_recorder /app

EXPOSE 8080

USER ${ID}

ENTRYPOINT ["tini", "--", "/app/target/debug/dummy_recorder"]
