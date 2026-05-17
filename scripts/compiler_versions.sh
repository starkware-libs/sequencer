#!/bin/env bash
# Reads compiler binary versions from version files (the single source of truth).
# Source this script to get $SIERRA_COMPILE_VERSION and $NATIVE_COMPILE_VERSION.

COMPILER_VERSIONS_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="$(cd "${COMPILER_VERSIONS_SCRIPT_DIR}/.." && pwd)"

# Reads and validates a semver-shaped version from a one-line text file.
# Trims surrounding whitespace; rejects anything that isn't `<num>.<num>.<num>`
# with an optional `-<pre-release>` suffix. The validation matters because the
# returned value flows into both `cargo install --version "$VAR"` and per-version
# install-root paths; a whitespace-separated value would be split by cargo's
# argument parser and `..` would let path construction escape the install root.
_compiler_versions_semver_re='^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$'
_compiler_versions_read() {
    # Sets the named-by-arg-1 variable to the validated version read from arg 2.
    # Returns 1 (and prints to stderr) on a malformed file. The caller checks
    # the return code; we cannot rely on `exit` because this function is
    # invoked from `source compiler_versions.sh`, where an exit inside a
    # subshell wouldn't propagate to the parent.
    local out_var=$1 file=$2 raw value
    raw=$(<"$file")
    value=$(printf '%s' "$raw" | tr -d '[:space:]')
    if [[ ! "$value" =~ $_compiler_versions_semver_re ]]; then
        echo "Error: invalid version in $file: '$raw'" >&2
        return 1
    fi
    printf -v "$out_var" '%s' "$value"
}

_compiler_versions_read SIERRA_COMPILE_VERSION \
    "$REPO_ROOT/crates/apollo_infra_utils/src/cairo_compiler_version.txt" || return 1
_compiler_versions_read NATIVE_COMPILE_VERSION \
    "$REPO_ROOT/crates/apollo_compile_to_native/src/native_compiler_version.txt" || return 1
