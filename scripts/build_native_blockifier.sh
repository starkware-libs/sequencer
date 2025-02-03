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
    cargo +1.80 build --release -p native_blockifier --features "testing" || clean
    clean
    popd
}

build
