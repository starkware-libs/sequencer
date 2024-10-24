#!/bin/bash

# TODO: Revert clippy::unwrap_used to -D after fixing all the unwraps.
cargo clippy --all-targets --all-features $@ -- \
    -D future-incompatible \
    -D nonstandard-style \
    -D rust-2018-idioms \
    -D unused \
    -A clippy::unwrap_used \
    -A clippy::blocks_in_conditions  # This is because of a bug in tracing: https://github.com/tokio-rs/tracing/issues/2876
