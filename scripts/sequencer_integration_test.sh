#!/bin/env bash

cargo build --bin starknet_sequencer_node
cargo run --bin sequencer_node_end_to_end_integration_test
