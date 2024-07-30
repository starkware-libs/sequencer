#!/bin/env bash
set -e

docker_image_name=blockifier-ci
docker build . -t ${docker_image_name} --file blockifier.Dockerfile

docker run \
    --rm \
    --net host \
    -e CARGO_HOME=${HOME}/.cargo \
    -u $UID \
    -v /tmp:/tmp \
    -v "${HOME}:${HOME}" \
    --workdir ${PWD}/crates/native_blockifier \
    ${docker_image_name} \
    cargo build --release --features "testing"
