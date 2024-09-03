#!/bin/env bash
set -e

function clean() {
    echo "Cleaning up..."
    deactivate || true
    rm -rf venv || true
}


function build() {
    echo "Building..."
    pushd crates/native_blockifier
    pypy3.9 -m venv venv
    source venv/bin/activate
    export MLIR_SYS_180_PREFIX=/usr/lib/llvm-18/
    export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18/
    export TABLEGEN_180_PREFIX=/usr/lib/llvm-18/
    cargo build --release -p native_blockifier --features "testing" || clean
    clean
    popd
}

build
