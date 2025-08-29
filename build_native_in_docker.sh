#!/bin/env bash
set -e

# Enables build-x for older versions of docker.
export DOCKER_BUILDKIT=1

docker_image_name=sequencer-ci
dockerfile_path="docker-ci/images/${docker_image_name}.Dockerfile"

docker build . --build-arg USER_UID=$UID -t ${docker_image_name} --file ${dockerfile_path}

# Set LLVM environment variables for cairo-native compilation
export MLIR_SYS_190_PREFIX=/usr/lib/llvm-19
export LLVM_SYS_191_PREFIX=/usr/lib/llvm-19
export TABLEGEN_190_PREFIX=/usr/lib/llvm-19

docker run \
    --rm \
    --net host \
    -e CARGO_HOME="${HOME}/.cargo" \
    -e MLIR_SYS_190_PREFIX="${MLIR_SYS_190_PREFIX}" \
    -e LLVM_SYS_191_PREFIX="${LLVM_SYS_191_PREFIX}" \
    -e TABLEGEN_190_PREFIX="${TABLEGEN_190_PREFIX}" \
    -u $UID \
    -v /tmp:/tmp \
    -v "${HOME}:${HOME}" \
    --workdir "${PWD}" \
    ${docker_image_name} \
    "$@"
