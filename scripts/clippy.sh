#!/bin/bash

cargo clippy "$@" --all-targets --all-features -- -Aclippy::blocks_in_conditions
