#!/bin/env bash

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

# Log a step with a visible separator for CI readability.
function log_step() {
    echo ""
    echo "========================================"
    echo "[install_build_tools] $1"
    echo "========================================"
}

# Retry apt-get update with cache cleanup to handle transient mirror sync issues.
function apt_update_with_retry() {
    local max_attempts=5
    local attempt=1
    local delay=5

    while [ $attempt -le $max_attempts ]; do
        echo "apt-get update attempt $attempt of $max_attempts..."
        if apt-get update; then
            echo "apt-get update succeeded on attempt $attempt"
            return 0
        fi

        echo "apt-get update failed on attempt $attempt"

        if [ $attempt -lt $max_attempts ]; then
            echo "Cleaning apt cache and retrying in ${delay}s..."
            rm -rf /var/lib/apt/lists/*
            sleep $delay
            delay=$((delay * 2))
        fi

        attempt=$((attempt + 1))
    done

    echo "apt-get update failed after $max_attempts attempts"
    return 1
}

# Retry apt-get install to handle transient network issues.
function apt_install_with_retry() {
    local max_attempts=5
    local attempt=1
    local delay=5

    while [ $attempt -le $max_attempts ]; do
        echo "apt-get install attempt $attempt of $max_attempts..."
        if apt-get install "$@"; then
            echo "apt-get install succeeded on attempt $attempt"
            return 0
        fi

        echo "apt-get install failed on attempt $attempt"

        if [ $attempt -lt $max_attempts ]; then
            echo "Retrying in ${delay}s..."
            sleep $delay
            delay=$((delay * 2))
        fi

        attempt=$((attempt + 1))
    done

    echo "apt-get install failed after $max_attempts attempts"
    return 1
}

function install_common_packages() {
    log_step "Installing common packages (build-essential, clang, curl, etc.)..."
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
    log_step "Common packages installed successfully"
}

function install_pypy() {
    log_step "Installing PyPy 3.9..."
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
    log_step "PyPy 3.9 installed successfully"
}

function install_rust() {
    log_step "Installing Rust via rustup..."
    curl https://sh.rustup.rs -sSf | sh -s -- -y
    log_step "Rust installed successfully"
}

function install_cargo_tools() {
    log_step "Installing cargo-insta..."
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mitsuhiko/insta/releases/download/1.42.0/cargo-insta-installer.sh | sh
    log_step "cargo-insta installed successfully"
}

cd "$(dirname "$0")"

log_step "Starting build tools installation..."

install_common_packages

log_step "Starting parallel installations (PyPy, Rust, cargo tools)..."
install_pypy &
install_rust &
install_cargo_tools &
wait
log_step "Parallel installations completed"

log_step "Running dependencies.sh..."
./dependencies.sh

log_step "All build tools installed successfully!"
