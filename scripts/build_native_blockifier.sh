#!/bin/env bash
set -e

function clean() {
    echo "Cleaning up..."
    deactivate || true
    rm -rf venv || true
}

pypy3.9 -m venv venv
source venv/bin/activate
cargo build --release -p native_blockifier --features "testing" || clean
clean
