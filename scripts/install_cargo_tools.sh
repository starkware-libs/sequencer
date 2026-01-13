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

function install_cargo_tools() {
    log_step "install_build_tools" "Installing cargo-insta..."
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mitsuhiko/insta/releases/download/1.42.0/cargo-insta-installer.sh | sh
    log_step "install_build_tools" "cargo-insta installed successfully"
    log_step "install_build_tools" "Installing cargo-machete..."
    cargo install cargo-machete --version 0.9.1
    log_step "install_build_tools" "cargo-machete installed successfully"
    log_step "install_build_tools" "Installing cargo-nextest..."
    cargo install cargo-nextest --version 0.9.113
    log_step "install_build_tools" "cargo-nextest installed successfully"
    log_step "install_build_tools" "Installing taplo-cli..."
    cargo install taplo-cli --version 0.9.3
    log_step "install_build_tools" "taplo-cli installed successfully"
    log_step "install_build_tools" "Installing cargo-deny..."
    cargo install cargo-deny --version 0.16.2
    log_step "install_build_tools" "cargo-deny installed successfully"
}

install_cargo_tools
