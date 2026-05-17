#!/bin/env bash
# Installs Sierra compiler binaries (starknet-sierra-compile, starknet-native-compile).
# Versions are read from plain text files (the single source of truth for both Rust and shell).
#
# Each version is installed under its own --root so multiple versions can coexist:
#   ${CARGO_TOOLS_ROOT:-$CARGO_HOME/tools}/<binary>-<version>/bin/<binary>
# The script does not put anything on $PATH; callers that need the binary on
# $PATH should use --dest.
#
# Usage:
#   scripts/install_compiler_binaries.sh                              # Install both
#   scripts/install_compiler_binaries.sh --sierra                     # Sierra only
#   scripts/install_compiler_binaries.sh --native                     # Native only
#   scripts/install_compiler_binaries.sh --dest /path/to/dir          # Stage at fixed path
#   scripts/install_compiler_binaries.sh --auto-install-llvm          # If LLVM 19 missing,
#                                                                     # run install_llvm19.sh
#
# --dest stages a copy at a caller-specified fixed path. Use for artifact
# pipelines that require a stable known location (e.g. binaries to be uploaded
# as build artifacts with a fixed object key).
#
# --auto-install-llvm opts into running scripts/install_llvm19.sh (which uses
# sudo apt) when LLVM 19 is missing. Default is to fail with a clear message;
# this avoids surprising callers with a system-package install side-effect.
#
# Stdout: absolute path of each installed binary, one per line.
# Stderr: progress logs (so stdout stays parseable).

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

# Source compiler version variables (provides $REPO_ROOT, $SIERRA_COMPILE_VERSION,
# $NATIVE_COMPILE_VERSION). The sourced script validates the version strings.
source "${SCRIPT_DIR}/compiler_versions.sh"

# Parse arguments.
INSTALL_SIERRA=false
INSTALL_NATIVE=false
DEST_DIR=""
AUTO_INSTALL_LLVM=false
explicit_selection=false
while [ $# -gt 0 ]; do
    case "$1" in
        --sierra) INSTALL_SIERRA=true; explicit_selection=true ;;
        --native) INSTALL_NATIVE=true; explicit_selection=true ;;
        --dest) DEST_DIR="$2"; shift ;;
        --auto-install-llvm) AUTO_INSTALL_LLVM=true ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
    shift
done
if ! $explicit_selection; then
    INSTALL_SIERRA=true
    INSTALL_NATIVE=true
fi

CARGO_TOOLS_ROOT="${CARGO_TOOLS_ROOT:-${CARGO_HOME:-$HOME/.cargo}/tools}"

# LLVM/MLIR env vars are normally set by .cargo/config.toml, but `cargo install`
# runs outside the workspace so they must be exported explicitly. We read the
# three known LLVM-tooling variables from an explicit allow-list and validate
# each value: cargo install runs untrusted build scripts, so any env var
# leakage from this function would let a malicious .cargo/config.toml change
# influence the install environment (e.g. LD_PRELOAD-adjacent attacks).
readonly _LLVM_ENV_VARS=(LLVM_SYS_191_PREFIX MLIR_SYS_190_PREFIX TABLEGEN_190_PREFIX)
readonly _LLVM_PATH_RE='^[A-Za-z0-9_/.:-]+$'
function export_llvm_env_vars() {
    # If the caller already provided all three vars in the environment (e.g.
    # Dockerfiles use `ENV`), trust them and skip the config.toml lookup.
    # Cargo install will receive them via the normal inheritance.
    local name all_set=true
    for name in "${_LLVM_ENV_VARS[@]}"; do
        if [ -z "${!name+x}" ]; then
            all_set=false
            break
        fi
    done
    if $all_set; then
        return 0
    fi

    # Otherwise read them from .cargo/config.toml. The fields are constrained
    # to a known allow-list (no greedy regex), and each value is validated
    # against a path-shape pattern: cargo install runs untrusted build
    # scripts, so leakage from this function would let a malicious config.toml
    # influence the install environment (e.g. LD_PRELOAD-adjacent attacks).
    local config_file="$REPO_ROOT/.cargo/config.toml"
    if [ ! -f "$config_file" ]; then
        echo "Error: missing LLVM env vars and ${config_file} not found." >&2
        echo "       Set ${_LLVM_ENV_VARS[*]} in the environment, or run from a workspace with .cargo/config.toml." >&2
        exit 1
    fi
    local value
    for name in "${_LLVM_ENV_VARS[@]}"; do
        value=$(sed -nE "s/^${name}[[:space:]]*=[[:space:]]*\"([^\"]*)\".*/\\1/p" "$config_file")
        if [ -z "$value" ]; then
            echo "Error: ${name} not found in ${config_file}" >&2
            exit 1
        fi
        if [[ ! "$value" =~ $_LLVM_PATH_RE ]]; then
            echo "Error: ${name} has unexpected value in ${config_file}: '${value}'" >&2
            exit 1
        fi
        export "$name=$value"
    done
}

# Atomic copy: write to a temp file in the destination directory then rename.
# Avoids a parallel reader catching a half-written binary.
function atomic_install() {
    local src=$1 dst=$2
    mkdir -p "$(dirname "$dst")"
    local tmp="${dst}.tmp.$$"
    cp "$src" "$tmp"
    mv -f "$tmp" "$dst"
}

# Install <binary> at <version> into a per-version --root.
# Reuses an existing install (caches across branch swaps); stages a copy at
# caller-requested DEST_DIR if requested; prints the absolute installed path
# on stdout.
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

    # Optional caller-requested copy at a fixed path (artifact pipelines).
    if [ -n "$DEST_DIR" ]; then
        atomic_install "$versioned_binary" "$DEST_DIR/${binary_name}"
    fi

    echo "$versioned_binary"
}

if $INSTALL_SIERRA; then
    install_compiler_if_needed "starknet-sierra-compile" "$SIERRA_COMPILE_VERSION"
fi

if $INSTALL_NATIVE; then
    # starknet-native-compile requires LLVM 19. By default we fail with a clear
    # message rather than silently invoking sudo apt; pass --auto-install-llvm
    # to opt into the system-level install via install_llvm19.sh.
    if ! command -v llvm-config-19 &>/dev/null; then
        if $AUTO_INSTALL_LLVM; then
            log_step "install_compiler_binaries" "LLVM 19 not found; --auto-install-llvm set, running install_llvm19.sh..." >&2
            "${SCRIPT_DIR}/install_llvm19.sh" >&2
        else
            echo "Error: LLVM 19 not installed (llvm-config-19 not on PATH)." >&2
            echo "       Run scripts/install_llvm19.sh first, or pass --auto-install-llvm to run it now." >&2
            exit 1
        fi
    fi
    export_llvm_env_vars
    install_compiler_if_needed "starknet-native-compile" "$NATIVE_COMPILE_VERSION"
fi
