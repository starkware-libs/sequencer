#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
source "${SCRIPT_DIR}/cargo_tool_utils.sh"

TOOLCHAIN=$(verify_and_return_fmt_toolchain)

echo "Running cargo fmt with toolchain ${TOOLCHAIN}"
cargo +"${TOOLCHAIN}" fmt --all -- "$@"
