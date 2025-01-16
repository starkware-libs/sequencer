#!/bin/env bash

killall starknet_sequencer_node
cargo build --bin starknet_sequencer_node
cargo run --bin sequencer_node_end_to_end_integration_test
