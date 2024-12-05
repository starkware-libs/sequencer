#!/bin/env bash
set -e

function clean() {
    echo "Cleaning up..."
    deactivate || true
    rm -rf venv || true
}

function init_submodule() {
    echo "Initializing submodule..."
    git submodule update --init --recursive
}

function build() {
    echo "Building..."
    pypy3.9 -m venv /tmp/venv
    source /tmp/venv/bin/activate
    cargo build --release -p native_blockifier --features "testing" || clean
    clean
}

init_submodule &&
build
