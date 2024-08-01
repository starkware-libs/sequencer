#!/bin/bash

# Install toolchain if missing (local run).
TOOLCHAIN="stable"
rustup toolchain list | grep -q ${TOOLCHAIN} || rustup toolchain install ${TOOLCHAIN}

cargo +${TOOLCHAIN} fmt --all -- "$@"
