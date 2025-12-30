#!/bin/bash

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

# Log a step with a visible separator for CI readability.
function log_step() {
    echo ""
    echo "========================================"
    echo "[dependencies] $1"
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

function install_essential_deps_linux() {
    log_step "Installing essential Linux dependencies..."
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
    log_step "Essential Linux dependencies installed successfully"
}

function setup_llvm_deps() {
    log_step "Setting up LLVM 19 dependencies..."
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
    log_step "LLVM 19 dependencies setup completed"
}

function main() {
    log_step "Starting dependencies installation..."
    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    setup_llvm_deps
    log_step "All dependencies installed successfully!"
}

main "$@"
