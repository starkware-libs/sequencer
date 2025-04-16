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
    pip install scripts/requirements.txt
    rustup toolchain install
    cargo build -p starknet_committer_and_os_cli -r --bin starknet_committer_and_os_cli || ret=$?
    clean
    return $ret
}

build
