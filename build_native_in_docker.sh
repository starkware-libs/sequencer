#!/bin/env bash
set -e

docker_image_name=sequencer-ci
dockerfile_path="ci/images/${docker_image_name}.Dockerfile"

docker build . --build-arg USER_UID=$UID -t ${docker_image_name} --file ${dockerfile_path}

docker run \
    --rm \
    --net host \
    -e CARGO_HOME="${HOME}/.cargo" \
    -u $UID \
    -v /tmp:/tmp \
    -v "${HOME}:${HOME}" \
    --workdir "${PWD}" \
    ${docker_image_name} \
    "$@"
