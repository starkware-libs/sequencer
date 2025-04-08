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
    clean
    return $ret
}

build
