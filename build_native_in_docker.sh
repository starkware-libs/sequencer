#!/bin/env bash
set -e

docker_image_name=sequancer-ci

(
    cd scripts
    docker build . -t ${docker_image_name} --file sequancer-ci.Dockerfile
)

docker run \
    --rm \
    --net host \
    -e CARGO_HOME=${HOME}/.cargo \
    -u $UID \
    -v /tmp:/tmp \
    -v "${HOME}:${HOME}" \
    --workdir ${PWD} \
    ${docker_image_name} \
    "$@"
