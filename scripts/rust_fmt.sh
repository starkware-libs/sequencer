#!/bin/bash

# Install toolchain if missing (local run).
TOOLCHAIN="nightly-2024-04-29"

function install_rustfmt() {
    rustup toolchain install ${TOOLCHAIN}
    rustup component add --toolchain ${TOOLCHAIN} rustfmt
}

rustup toolchain list | grep -q ${TOOLCHAIN} || install_rustfmt

cargo +${TOOLCHAIN} fmt $@ -- --check
