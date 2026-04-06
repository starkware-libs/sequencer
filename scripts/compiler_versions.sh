#!/bin/env bash
# Reads compiler binary versions from version files (the single source of truth).
# Source this script to get $SIERRA_COMPILE_VERSION and $NATIVE_COMPILE_VERSION.

COMPILER_VERSIONS_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="$(cd "${COMPILER_VERSIONS_SCRIPT_DIR}/.." && pwd)"

SIERRA_COMPILE_VERSION=$(cat "$REPO_ROOT/crates/apollo_infra_utils/src/cairo_compiler_version.txt")
NATIVE_COMPILE_VERSION=$(cat "$REPO_ROOT/crates/apollo_compile_to_native/src/native_compiler_version.txt")
