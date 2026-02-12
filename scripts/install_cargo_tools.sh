#!/bin/env bash

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

# Source common apt utilities
# Handle both cases: script in subdirectory or current directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
if [ -f "${SCRIPT_DIR}/apt_utils.sh" ]; then
    source "${SCRIPT_DIR}/apt_utils.sh"
elif [ -f "./apt_utils.sh" ]; then
    source "./apt_utils.sh"
else
    echo "Error: apt_utils.sh not found in ${SCRIPT_DIR} or current directory" >&2
    exit 1
fi

# Source common cargo utilities.
if [ -f "${SCRIPT_DIR}/cargo_tool_utils.sh" ]; then
    source "${SCRIPT_DIR}/cargo_tool_utils.sh"
elif [ -f "./cargo_tool_utils.sh" ]; then
    source "./cargo_tool_utils.sh"
else
    echo "Error: cargo_tool_utils.sh not found in ${SCRIPT_DIR} or current directory" >&2
    exit 1
fi

# Install a cargo tool only if needed (not installed or different version)
# Args: version_cmd, crate_name, version
function install_cargo_tool_if_needed() {
    local version_cmd=$1
    local crate=$2
    local version=$3
    local current
    current=$($version_cmd 2>/dev/null | grep -oP '\d+\.\d+\.\d+' | head -1) || true
    if [ "$current" = "$version" ]; then
        log_step "install_build_tools" "$crate $version already installed, skipping"
    else
        if [ -n "$current" ]; then
            log_step "install_build_tools" "Replacing $crate $current with $version..."
        else
            log_step "install_build_tools" "Installing $crate $version..."
        fi
        cargo install "$crate" --version "$version" --force
        log_step "install_build_tools" "$crate installed successfully"
    fi
}

function install_cargo_tools() {
    echo "Installing cargo rustfmt toolchain..."
    verify_and_return_fmt_toolchain
    echo "Cargo rustfmt toolchain installed successfully"

    log_step "install_build_tools" "Installing cargo-insta..."
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mitsuhiko/insta/releases/download/1.42.0/cargo-insta-installer.sh | sh
    log_step "install_build_tools" "cargo-insta installed successfully"

    install_cargo_tool_if_needed "cargo machete --version" "cargo-machete" "0.9.1"
    install_cargo_tool_if_needed "cargo nextest --version" "cargo-nextest" "0.9.113"
    install_cargo_tool_if_needed "taplo --version" "taplo-cli" "0.9.3"
    install_cargo_tool_if_needed "cargo deny --version" "cargo-deny" "0.16.2"
}

install_cargo_tools
