#!/bin/env bash
# Reads compiler binary versions from version files (the single source of truth).
# Source this script to get $SIERRA_COMPILE_VERSION and $NATIVE_COMPILE_VERSION.

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)")"

SIERRA_COMPILE_VERSION=$(cat "$REPO_ROOT/crates/apollo_infra_utils/src/cairo_compiler_version.txt")
NATIVE_COMPILE_VERSION=$(cat "$REPO_ROOT/crates/apollo_compile_to_native/src/native_compiler_version.txt")
