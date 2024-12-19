#!/bin/env bash
set -e

function build() {
    echo "Building..."
    source /tmp/venv/bin/activate
    cargo build --release -p native_blockifier
}

build