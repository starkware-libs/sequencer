#!/bin/bash

set -euo pipefail

# TODO(Dori): Since it is not possible to specify multiple toolchains in a single
#   rust-toolchain.toml file (and directing that specific cargo commands should run with the nightly
#   toolchain), we need to find a nice way to cache the nightly toolchain installation in the CI,
#   without defining the specific nightly version in multiple places.
TOOLCHAIN=nightly-2024-04-29

function install_rustfmt() {
    rustup toolchain install "${TOOLCHAIN}"
    rustup component add --toolchain "${TOOLCHAIN}" rustfmt
}

rustup toolchain list | grep -q "${TOOLCHAIN}" || install_rustfmt

echo "Running cargo fmt with toolchain ${TOOLCHAIN}"
cargo +"${TOOLCHAIN}" fmt --all -- "$@"
