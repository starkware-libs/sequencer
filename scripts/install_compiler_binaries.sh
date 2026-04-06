#!/bin/env bash
# Installs Sierra compiler binaries (starknet-sierra-compile, starknet-native-compile).
# Versions are read from plain text files (the single source of truth for both Rust and shell).
#
# Each version is installed under its own --root so multiple versions can coexist:
#   ${CARGO_TOOLS_ROOT:-$CARGO_HOME/tools}/<binary>-<version>/bin/<binary>
# A copy is also placed in $CARGO_HOME/bin so the binary is discoverable on PATH.
#
# Usage:
#   scripts/install_compiler_binaries.sh                              # Install both
#   scripts/install_compiler_binaries.sh --sierra                     # Sierra only
#   scripts/install_compiler_binaries.sh --native                     # Native only
#   scripts/install_compiler_binaries.sh --dest /usr/local/bin        # Also copy to dest
#
# --dest copies each installed binary into the given directory (useful in Docker
# so the final stage can COPY from a known path regardless of CARGO_HOME).
#
# Stdout: absolute path of each installed binary, one per line.
# Stderr: progress logs (so stdout stays parseable).
#
# Self-recovers if LLVM 19 is missing for the native compiler by invoking
# scripts/install_llvm19.sh.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

# Source common apt utilities (for log_step).
if [ -f "${SCRIPT_DIR}/apt_utils.sh" ]; then
    source "${SCRIPT_DIR}/apt_utils.sh"
elif [ -f "./apt_utils.sh" ]; then
    source "./apt_utils.sh"
else
    echo "Error: apt_utils.sh not found in ${SCRIPT_DIR} or current directory" >&2
    exit 1
fi

# Source compiler version variables.
source "${SCRIPT_DIR}/compiler_versions.sh"

# Parse arguments.
INSTALL_SIERRA=false
INSTALL_NATIVE=false
DEST_DIR=""
explicit_selection=false
while [ $# -gt 0 ]; do
    case "$1" in
        --sierra) INSTALL_SIERRA=true; explicit_selection=true ;;
        --native) INSTALL_NATIVE=true; explicit_selection=true ;;
        --dest) DEST_DIR="$2"; shift ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
    shift
done
if ! $explicit_selection; then
    INSTALL_SIERRA=true
    INSTALL_NATIVE=true
fi

CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
CARGO_TOOLS_ROOT="${CARGO_TOOLS_ROOT:-${CARGO_HOME_DIR}/tools}"

# LLVM/MLIR env vars are normally set by .cargo/config.toml, but `cargo install`
# runs outside the workspace so they must be exported explicitly. We parse them
# out of .cargo/config.toml dynamically so adding/removing variables there
# automatically propagates here.
function export_llvm_env_vars() {
    local config_file="$REPO_ROOT/.cargo/config.toml"
    [ -f "$config_file" ] || return 0
    local line var_name var_value
    while IFS= read -r line; do
        var_name=$(echo "$line" | sed -n 's/^\([A-Z0-9_]*\) = .*/\1/p')
        var_value=$(echo "$line" | sed -n 's/^[A-Z0-9_]* = "\(.*\)"/\1/p')
        if [ -n "$var_name" ] && [ -n "$var_value" ]; then
            export "$var_name=$var_value"
        fi
    done < <(grep -E '^(LLVM_SYS|MLIR_SYS|TABLEGEN)' "$config_file")
}

# Install <binary> at <version> into a per-version --root.
# Reuses an existing install (caches across branch swaps); copies to PATH and
# any caller-requested DEST_DIR; prints the absolute installed path on stdout.
function install_compiler_if_needed() {
    local binary_name=$1
    local version=$2
    local install_root="${CARGO_TOOLS_ROOT}/${binary_name}-${version}"
    local versioned_binary="${install_root}/bin/${binary_name}"

    if [ -x "$versioned_binary" ]; then
        log_step "install_compiler_binaries" "${binary_name} ${version} already installed, skipping" >&2
    else
        log_step "install_compiler_binaries" "Installing ${binary_name} ${version}..." >&2
        cargo install --locked --root "$install_root" "$binary_name" --version "$version" >&2
        log_step "install_compiler_binaries" "Installed ${binary_name} ${version}" >&2
    fi

    # Place in $CARGO_HOME/bin so plain `Command::new("$binary_name")` finds it on PATH.
    mkdir -p "${CARGO_HOME_DIR}/bin"
    cp "$versioned_binary" "${CARGO_HOME_DIR}/bin/${binary_name}"

    # Optional caller-requested copy (e.g. for Docker COPY or workflow upload).
    if [ -n "$DEST_DIR" ]; then
        mkdir -p "$DEST_DIR"
        cp "$versioned_binary" "$DEST_DIR/${binary_name}"
    fi

    echo "$versioned_binary"
}

if $INSTALL_SIERRA; then
    install_compiler_if_needed "starknet-sierra-compile" "$SIERRA_COMPILE_VERSION"
fi

if $INSTALL_NATIVE; then
    # starknet-native-compile requires LLVM 19; auto-install if missing instead
    # of failing with a cryptic tblgen build error.
    if ! command -v llvm-config-19 &>/dev/null; then
        log_step "install_compiler_binaries" "LLVM 19 not found, running install_llvm19.sh..." >&2
        "${SCRIPT_DIR}/install_llvm19.sh" >&2
    fi
    export_llvm_env_vars
    install_compiler_if_needed "starknet-native-compile" "$NATIVE_COMPILE_VERSION"
fi
