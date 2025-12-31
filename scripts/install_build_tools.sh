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

function install_common_packages() {
    log_step "install_build_tools" "Installing common packages (build-essential, clang, curl, etc.)..."
    $SUDO  bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt_install_with_retry -y \
            build-essential \
            clang \
            curl \
            gnupg \
            libzstd-dev \
            python3-dev \
            python3-venv \
            sudo \
            tzdata \
            wget
        '
    log_step "install_build_tools" "Common packages installed successfully"
}

function install_pypy() {
    log_step "install_build_tools" "Installing PyPy 3.9..."
    pushd /opt
    $SUDO bash -c '
        echo "Downloading PyPy 3.9..."
        curl -Lo pypy3.9-v7.3.11-linux64.tar.bz2 https://downloads.python.org/pypy/pypy3.9-v7.3.11-linux64.tar.bz2
        echo "Extracting PyPy..."
        tar -xf pypy3.9-v7.3.11-linux64.tar.bz2
        rm pypy3.9-v7.3.11-linux64.tar.bz2
        chmod +x pypy3.9-v7.3.11-linux64/bin/pypy3

        echo "Creating PyPy symlinks..."
        if [ -L /usr/local/bin/pypy3.9 ]; then
            unlink /usr/local/bin/pypy3.9
        fi

        ln -s /opt/pypy3.9-v7.3.11-linux64/bin/pypy3 /usr/local/bin/pypy3.9

        if [ -L /opt/pypy3.9 ]; then
            unlink /opt/pypy3.9
        fi

        ln -s /opt/pypy3.9-v7.3.11-linux64 /opt/pypy3.9
        echo "Installing pip and wheel for PyPy..."
        pypy3.9 -m ensurepip
        pypy3.9 -m pip install wheel
        '
    popd
    log_step "install_build_tools" "PyPy 3.9 installed successfully"
}

function install_rust() {
    log_step "install_build_tools" "Installing Rust via rustup..."
    curl https://sh.rustup.rs -sSf | sh -s -- -y
    log_step "install_build_tools" "Rust installed successfully"
}

function install_cargo_tools() {
    log_step "install_build_tools" "Installing cargo-insta..."
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mitsuhiko/insta/releases/download/1.42.0/cargo-insta-installer.sh | sh
    log_step "install_build_tools" "cargo-insta installed successfully"
}

cd "$(dirname "$0")"

log_step "install_build_tools" "Starting build tools installation..."

install_common_packages

log_step "install_build_tools" "Starting parallel installations (PyPy, Rust, cargo tools)..."
install_pypy &
install_rust &
install_cargo_tools &
wait
log_step "install_build_tools" "Parallel installations completed"

log_step "install_build_tools" "Running dependencies.sh..."
./dependencies.sh

log_step "install_build_tools" "All build tools installed successfully!"
