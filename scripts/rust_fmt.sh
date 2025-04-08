#!/bin/bash

set -euo pipefail

if [[ -n "${CI:-}" ]]; then
  echo "This script should not be run in a CI environment, as it installs toolchains out of cache."
  exit 1
fi

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
TOOLCHAIN=$(grep "EXTRA_RUST_TOOLCHAINS:" "${SCRIPT_DIR}"/../.github/workflows/main.yml  | awk '{print $2}')

function install_rustfmt() {
    rustup toolchain install "${TOOLCHAIN}"
    rustup component add --toolchain "${TOOLCHAIN}" rustfmt
}

rustup toolchain list | grep -q "${TOOLCHAIN}" || install_rustfmt

cargo +"${TOOLCHAIN}" fmt --all -- "$@"
