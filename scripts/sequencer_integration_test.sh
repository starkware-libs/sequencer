#!/bin/env bash

sudo killall starknet_sequencer_node
cargo build --bin starknet_sequencer_node
cargo run --bin sequencer_node_end_to_end_positive_flow_integration_test
