#!/bin/bash

# To auto-fix formatting install taplo (`cargo install taplo-cli`) and run `taplo format`.
# Formatting options can be configured in the root `taplo.toml` file.

taplo format --check --diff 1>&2
