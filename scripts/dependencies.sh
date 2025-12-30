#!/bin/bash

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

function install_essential_deps_linux() {
    log_step "dependencies" "Installing essential Linux dependencies..."
    $SUDO bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && apt_install_with_retry -y \
            ca-certificates \
            curl \
            git \
            gnupg \
            jq \
            libssl-dev \
            lsb-release \
            pkg-config \
            ripgrep \
            software-properties-common \
            zstd \
            wget \
            lld
  '
    log_step "dependencies" "Essential Linux dependencies installed successfully"
}

function setup_llvm_deps() {
    log_step "dependencies" "Setting up LLVM 19 dependencies..."
    case "$(uname)" in
    Darwin)
        echo "Detected macOS, using Homebrew..."
        brew update
        brew install llvm@19
        ;;
    Linux)
        echo "Detected Linux, using apt..."
        $SUDO bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        echo "Downloading LLVM installation script..."
        curl https://apt.llvm.org/llvm.sh -Lo llvm.sh
        echo "Running LLVM 19 installation script..."
        bash ./llvm.sh 19 all
        rm -f ./llvm.sh
        echo "Installing LLVM-related packages (MLIR, Polly, etc.)..."
        apt_update_with_retry && apt_install_with_retry -y \
            libgmp3-dev \
            libmlir-19-dev \
            libpolly-19-dev \
            libzstd-dev \
            mlir-19-tools
        '
        ;;
    *)
        echo "Error: Unsupported operating system"
        exit 1
        ;;
    esac
    log_step "dependencies" "LLVM 19 dependencies setup completed"
}

function main() {
    log_step "dependencies" "Starting dependencies installation..."
    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    setup_llvm_deps
    log_step "dependencies" "All dependencies installed successfully!"
}

main "$@"
