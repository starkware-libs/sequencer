#!/bin/env bash
set -e

docker_image_name=sequencer-ci

SEQUENCER_DIR=${PWD}
(
    cd scripts
    docker build . -t ${docker_image_name} --file ${docker_image_name}.Dockerfile --build-arg SEQUENCER_DIR=${SEQUENCER_DIR}
)

echo "asdf uid: $UID"
echo "asdf build_native home: ${HOME}"
echo "asdf build_native pwd: ${PWD}"
echo "asdf build_native dock: ${docker_image_name}"

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
