#!/bin/bash
set -e
cargo run "$@" -p apollo_compile_to_casm --bin install_starknet_sierra_compile
cargo run "$@" -p apollo_compile_to_native --bin install_starknet_native_compile
