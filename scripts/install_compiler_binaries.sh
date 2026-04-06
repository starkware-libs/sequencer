#!/bin/env bash
# Installs Sierra compiler binaries (starknet-sierra-compile, starknet-native-compile).
# Versions are read from plain text files that are the single source of truth for both
# Rust code and this script.
#
# Called from install_cargo_tools.sh. Can also be run standalone (requires LLVM 19 for
# starknet-native-compile; run scripts/dependencies.sh first if needed).

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

# Source version files and shared utilities.
source "${SCRIPT_DIR}/compiler_versions.sh"
if [ -f "${SCRIPT_DIR}/apt_utils.sh" ]; then
    source "${SCRIPT_DIR}/apt_utils.sh"
elif [ -f "./apt_utils.sh" ]; then
    source "./apt_utils.sh"
fi

# Install a compiler binary only if needed (not installed or different version).
function install_compiler_if_needed() {
    local binary_name=$1
    local version=$2
    local current
    current=$($binary_name --version 2>/dev/null | grep -oP '\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?' | head -1) || true
    if [ "$current" = "$version" ]; then
        log_step "install_build_tools" "$binary_name $version already installed, skipping"
    else
        if [ -n "$current" ]; then
            log_step "install_build_tools" "Replacing $binary_name $current with $version..."
        else
            log_step "install_build_tools" "Installing $binary_name $version..."
        fi
        cargo install "$binary_name" --version "$version" --force
        log_step "install_build_tools" "$binary_name installed successfully"
    fi
}

# LLVM/MLIR env vars are normally set by .cargo/config.toml, but cargo install runs
# outside the workspace so they must be set explicitly.
if [ -f "$REPO_ROOT/.cargo/config.toml" ]; then
    eval "$(grep -E '(LLVM_SYS|MLIR_SYS|TABLEGEN)' "$REPO_ROOT/.cargo/config.toml" | sed 's/ = /=/' | tr -d '"')"
    export LLVM_SYS_191_PREFIX MLIR_SYS_190_PREFIX TABLEGEN_190_PREFIX
fi

install_compiler_if_needed "starknet-sierra-compile" "$SIERRA_COMPILE_VERSION"

# starknet-native-compile requires LLVM 19. If LLVM is not installed, print instructions
# instead of failing with a cryptic tblgen build error.
if command -v llvm-config-19 &>/dev/null; then
    install_compiler_if_needed "starknet-native-compile" "$NATIVE_COMPILE_VERSION"
else
    log_step "install_build_tools" "Skipping starknet-native-compile (LLVM 19 not found)."
    log_step "install_build_tools" "To install it: run 'scripts/dependencies.sh' first, then re-run this script."
fi
