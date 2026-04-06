#!/bin/env bash
# Installs Sierra compiler binaries (starknet-sierra-compile, starknet-native-compile).
# Versions are read from plain text files that are the single source of truth for both
# Rust code and this script.
#
# Usage:
#   scripts/install_compiler_binaries.sh           # Install both (requires LLVM 19)
#   scripts/install_compiler_binaries.sh --sierra   # Install starknet-sierra-compile only
#   scripts/install_compiler_binaries.sh --native   # Install starknet-native-compile only
#
# Prints the absolute path of each installed binary to stdout (one per line).
# Can be run standalone (requires LLVM 19 for native; run scripts/dependencies.sh first).

set -e

# Source common apt utilities.
# Handle both cases: script in subdirectory or current directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
if [ -f "${SCRIPT_DIR}/apt_utils.sh" ]; then
    source "${SCRIPT_DIR}/apt_utils.sh"
elif [ -f "./apt_utils.sh" ]; then
    source "./apt_utils.sh"
else
    echo "Error: apt_utils.sh not found in ${SCRIPT_DIR} or current directory" >&2
    exit 1
fi

# Source version files.
source "${SCRIPT_DIR}/compiler_versions.sh"

# Parse arguments.
INSTALL_SIERRA=false
INSTALL_NATIVE=false
if [ $# -eq 0 ]; then
    INSTALL_SIERRA=true
    INSTALL_NATIVE=true
fi
for arg in "$@"; do
    case "$arg" in
        --sierra) INSTALL_SIERRA=true ;;
        --native) INSTALL_NATIVE=true ;;
        *) echo "Unknown argument: $arg" >&2; exit 1 ;;
    esac
done

# LLVM/MLIR env vars are normally set by .cargo/config.toml, but cargo install runs
# outside the workspace so they must be set explicitly.
function export_llvm_env_vars() {
    local config_file="$REPO_ROOT/.cargo/config.toml"
    if [ ! -f "$config_file" ]; then
        return
    fi
    local line var_name var_value
    while IFS= read -r line; do
        var_name=$(echo "$line" | sed -n 's/^\([A-Z_]*\) = .*/\1/p')
        var_value=$(echo "$line" | sed -n 's/^[A-Z_]* = "\(.*\)"/\1/p')
        if [ -n "$var_name" ] && [ -n "$var_value" ]; then
            export "$var_name=$var_value"
        fi
    done < <(grep -E '^(LLVM_SYS|MLIR_SYS|TABLEGEN)' "$config_file")
}

# Install a compiler binary with multi-version support.
# Installs as <binary_name>-<version> and symlinks <binary_name> to the active version.
# Skips installation if the versioned binary already exists.
# Prints the absolute path of the installed binary to stdout.
function install_compiler_if_needed() {
    local binary_name=$1
    local version=$2
    local versioned_name="${binary_name}-${version}"

    if command -v "$versioned_name" &>/dev/null; then
        log_step "install_build_tools" "$versioned_name already installed, skipping"
    else
        log_step "install_build_tools" "Installing $versioned_name..."
        cargo install "$binary_name" --version "$version" --force
        # Rename to versioned path so multiple versions can coexist.
        mv "$(which "$binary_name")" "$(dirname "$(which "$binary_name")")/$versioned_name"
        log_step "install_build_tools" "$versioned_name installed successfully"
    fi

    # Symlink the active version.
    local bin_dir
    bin_dir="$(dirname "$(which "$versioned_name")")"
    ln -sf "$bin_dir/$versioned_name" "$bin_dir/$binary_name"

    # Print the absolute path of the installed binary.
    echo "$bin_dir/$binary_name"
}

if $INSTALL_SIERRA; then
    install_compiler_if_needed "starknet-sierra-compile" "$SIERRA_COMPILE_VERSION"
fi

if $INSTALL_NATIVE; then
    # starknet-native-compile requires LLVM 19. If LLVM is not installed, print instructions
    # instead of failing with a cryptic tblgen build error.
    if command -v llvm-config-19 &>/dev/null; then
        export_llvm_env_vars
        install_compiler_if_needed "starknet-native-compile" "$NATIVE_COMPILE_VERSION"
    else
        log_step "install_build_tools" "Skipping starknet-native-compile (LLVM 19 not found)."
        log_step "install_build_tools" "To install it: run 'scripts/dependencies.sh' first, then re-run this script."
    fi
fi
