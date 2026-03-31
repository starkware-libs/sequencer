#!/bin/env bash
# Extracts compiler binary versions from Rust source files (the single source of truth).
# Source this script to get $SIERRA_COMPILE_VERSION and $NATIVE_COMPILE_VERSION.

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)")"

SIERRA_COMPILE_VERSION=$(grep 'CAIRO1_COMPILER_VERSION' "$REPO_ROOT/crates/apollo_infra_utils/src/cairo_compiler_version.rs" | grep -oP '"\K[^"]+' | head -1)
NATIVE_COMPILE_VERSION=$(grep 'REQUIRED_CAIRO_NATIVE_VERSION' "$REPO_ROOT/crates/apollo_compile_to_native/src/constants.rs" | grep -oP '"\K[^"]+' | head -1)
