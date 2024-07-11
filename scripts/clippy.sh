#!/bin/bash

# clippy::blocks_in_conditions is allowed because of a bug in tracing:
# https://github.com/tokio-rs/tracing/issues/2876.
cargo clippy "$@" --all-targets --all-features -- -Aclippy::blocks_in_conditions
