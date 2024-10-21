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
        brew install llvm@19
        ;;
    Linux)
        $SUDO bash -c 'curl https://apt.llvm.org/llvm.sh -Lo llvm.sh
        bash ./llvm.sh 19 all
        rm -f ./llvm.sh
        apt update && apt install -y \
            libgmp3-dev \
            libmlir-19-dev \
            libpolly-19-dev \
            libzstd-dev \
            mlir-19-tools \
            lld
        '
        ;;
    *)
        echo "Error: Unsupported operating system"
        exit 1
        ;;
    esac
}

function compile_cairo_native_runtime() {
    TARGET_LIB_DIR="$1"
    # First we need to make sure Cargo exists
    if command -v cargo >/dev/null 2>&1; then
        echo "Rust is already installed with cargo available in PATH."
    else
        echo "cargo not found. Installing Rust..."
        if ! curl -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path; then
            echo >&2 "Failed to install Rust. Aborting."
            return 1
        fi
        # shellcheck disable=SC1090
        source "$HOME/.cargo/env" || {
            echo >&2 "Failed to source Rust environment. Aborting."
            return 1
        }
    fi

    # Then we clone and build the runtime from the repo
    git clone https://github.com/lambdaclass/cairo_native.git
    pushd ./cairo_native || exit 1
    git switch v0.2.0-alpha.2 --detach
    cargo build -p cairo-native-runtime --release --all-features --quiet
    popd || exit 1

    mv ./cairo_native/target/release/libcairo_native_runtime.a ${LIBCAIRO_NATIVE_DIR}/libcairo_native_runtime.a
    rm -rf ./cairo_native
}

function main() {
    # Set LIBCAIRO_NATIVE_DIR as first argument.
    # Assumes this script is in `sequencer/scripts/`
    # By default, copy to `sequencer/scripts/../crates/blockifier`
    # Used in `.github/actions/bootstrap/action.yml` and when calling manually.
    THIS_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
    DEFAULT_DIR="$THIS_DIR/../crates/blockifier"
    LIBCAIRO_NATIVE_DIR=${1:-"$DEFAULT_DIR"}

    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    setup_llvm_deps
    echo "LLVM dependencies installed successfully."

    compile_cairo_native_runtime "$LIBCAIRO_NATIVE_DIR"
    echo "Cairo Native runtime compiled successfully."
}

main "$@"

