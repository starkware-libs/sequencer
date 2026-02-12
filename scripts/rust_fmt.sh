#!/bin/bash

set -euo pipefail

source scripts/cargo_tool_utils.sh

TOOLCHAIN=$(verify_and_return_fmt_toolchain)

echo "Running cargo fmt with toolchain ${TOOLCHAIN}"
cargo +"${TOOLCHAIN}" fmt --all -- "$@"
