#!/bin/bash

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

function install_essential_deps_linux() {
    $SUDO bash -c '
        apt update && apt install -y \
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
            wget
  '
}

function setup_llvm_deps() {
    case "$(uname)" in
    Darwin)
        brew update
        brew install llvm@18

        export LIBRARY_PATH=/opt/homebrew/lib
        export MLIR_SYS_180_PREFIX="$(brew --prefix llvm@18)"
        export LLVM_SYS_181_PREFIX="$MLIR_SYS_180_PREFIX"
        export TABLEGEN_180_PREFIX="$MLIR_SYS_180_PREFIX"
        ;;
    Linux)
        $SUDO bash -c 'curl https://apt.llvm.org/llvm.sh -Lo llvm.sh
        bash ./llvm.sh 18 all
        apt update && apt install -y \
            libgmp3-dev \
            libmlir-18-dev \
            libpolly-18-dev \
            libzstd-dev \
            mlir-18-tools
        '
        ;;
    *)
        echo "Error: Unsupported operating system"
        exit 1
        ;;
    esac
}

function main() {
    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    setup_llvm_deps
    echo "LLVM and Cairo native runtime dependencies installed successfully."
}

main "$@"
