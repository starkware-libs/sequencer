#!/bin/env bash
set -e

image_name="sequencer-ci"

(
    cd scripts
    docker build . -t "${image_name}" --file "${image_name}.Dockerfile"
)

docker run \
    --rm \
    --net host \
    -e CARGO_HOME="${HOME}/.cargo" \
    -u "$UID" \
    -v /tmp:/tmp \
    -v "${HOME}:${HOME}" \
    --workdir "${PWD}" \
    "${image_name}" \
    "$@"
