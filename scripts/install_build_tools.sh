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

function install_common_packages() {
    log_step "install_build_tools" "Installing common packages (build-essential, clang, curl, etc.)..."
    $SUDO  bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt_install_with_retry -y \
            build-essential \
            clang \
            curl \
            gnupg \
            libssl-dev \
            libzstd-dev \
            pkg-config \
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
    # Source the cargo environment to add ~/.cargo/bin to PATH for this shell session.
    # This is required because rustup installs to ~/.cargo/bin which isn't in PATH yet.
    # The env file may not exist if rust was already installed.
    if [ -f "${HOME}/.cargo/env" ]; then
        source "${HOME}/.cargo/env"
    elif [ -d "${HOME}/.cargo/bin" ]; then
        export PATH="${HOME}/.cargo/bin:${PATH}"
    fi
    # Now that rustup is installed, we can install the cargo rustfmt toolchain.
    echo "Installing cargo rustfmt toolchain..."
    verify_and_return_fmt_toolchain
    echo "Cargo rustfmt toolchain installed successfully"
}

cd "$(dirname "$0")"

log_step "install_build_tools" "Starting build tools installation..."

install_common_packages

log_step "install_build_tools" "Installing PyPy and Rust..."
# Intentionally serial: rustup-init's multi-threaded component downloader has a
# known race (`thread 'CloseHandle' panicked at src/diskio/mod.rs: RecvError`)
# that's triggered or aggravated by concurrent I/O from parallel installers.
# When the race fires, rustup leaves a partial toolchain stub behind and later
# `cargo install` calls fail with "'cargo' is not installed for the toolchain".
install_pypy
install_rust
log_step "install_build_tools" "PyPy and Rust installed"

# Belt-and-suspenders: rustup can still leave the pinned toolchain partially
# installed (the `rust-toolchain.toml`-pinned channel as a stub without its
# components). Probe with `cargo --version` against the active toolchain; if it
# fails, force a reinstall to get the full component set.
log_step "install_build_tools" "Verifying project Rust toolchain is usable..."
pushd "${SCRIPT_DIR}/.." > /dev/null
if ! cargo --version > /dev/null 2>&1; then
    log_step "install_build_tools" "Project toolchain stub is missing components; forcing reinstall..."
    rustup toolchain install --force
fi
popd > /dev/null
log_step "install_build_tools" "Project Rust toolchain ready: $(rustc --version)"

log_step "install_build_tools" "Running install_cargo_tools.sh..."
${SCRIPT_DIR}/install_cargo_tools.sh
log_step "install_build_tools" "install_cargo_tools.sh completed"
log_step "install_build_tools" "Running dependencies.sh..."
./dependencies.sh

log_step "install_build_tools" "All build tools installed successfully!"
