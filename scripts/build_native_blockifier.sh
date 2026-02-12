#!/bin/env bash
set -e

function clean() {
    echo "Cleaning up..."
    deactivate || true
    rm -rf venv || true
}


function build() {
    ret=0
    echo "Building..."
    pypy3.9 -m venv /tmp/venv
    source /tmp/venv/bin/activate
    rustup toolchain install
    cargo build --release -p native_blockifier --features "cairo_native" || ret=$?
    if [ $ret -eq 0 ]; then
        cargo run --release -p apollo_compile_to_casm --bin install_starknet_sierra_compile || ret=$?
    fi
    if [ $ret -eq 0 ]; then
        cargo run --release -p apollo_compile_to_native --bin install_starknet_native_compile || ret=$?
    fi
    clean
    return $ret
}

build
