#!/bin/bash

# TODO: Revert clippy::unwrap_used to -D after fixing all the unwraps.
cargo clippy --workspace --all-targets --all-features -- \
    -D future-incompatible \
    -D nonstandard-style \
    -D rust-2018-idioms \
    -D unused \
