#!/bin/env bash

# Script for the entry point of sequencer-ci.Dockerfile.

# Set SEQUENCER_DIR as first argument.
[ "$#" -gt 0 ] || (echo "Path to sequencer repo required as an argument, $# arguments provided" ; exit 1)
SEQUENCER_DIR="$1"
remaining_args=${@:2}

# Building the docker image builds libcairo_native_runtime.a, but the sequencer repo may not exist (on the docker image).
# When running the github actions CI, `build_native_in_docker.sh` is called and mounts home.
# `-v "${HOME}:${HOME}"` mounts `/home/runner/work/sequencer`
# In `.github/workflows/committer_cli_push.yml`, `actions/checkout@v4` pulls the repo under home.
# Thus, when running docker, we need to grab the lib from the build step and move it under our new mounted directory.
# Final destination for `libcairo_native_runtime.a` committer_cli_push is `/home/runner/work/sequencer/sequencer/crates/blockifier/libcairo_native_runtime.a`.
function copy_cairo_native_lib() {
    SEQUENCER_DIR="$1"
    # Set TARGET_LIB_DIR as first argument, or by default the pwd.
    echo "Copying cairo native runtime library to blockifier crate"
    echo "SEQUENCER_DIR: ${SEQUENCER_DIR}"
    set -x
    cp /cairo_native/libcairo_native_runtime.a "${SEQUENCER_DIR}/crates/blockifier/libcairo_native_runtime.a"
    { set +x; } 2>/dev/null
}

copy_cairo_native_lib "${SEQUENCER_DIR}"

# Run the passed in command using remaining arguments
$remaining_args