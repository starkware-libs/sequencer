#!/bin/bash

set -euo pipefail

if [[ -n "${CI:-}" ]]; then
  echo "This script should not be run in a CI environment, as it installs toolchains out of cache."
  exit 1
fi

TOOLCHAIN=nightly-2024-04-29

function install_rustfmt() {
    rustup toolchain install "${TOOLCHAIN}"
    rustup component add --toolchain "${TOOLCHAIN}" rustfmt
}

rustup toolchain list | grep -q "${TOOLCHAIN}" || install_rustfmt

cargo +"${TOOLCHAIN}" fmt --all -- "$@"
