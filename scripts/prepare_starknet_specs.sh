#!/usr/bin/env bash
# Clones starknet-specs at the revision pinned in
# crates/starknet_transaction_prover/resources/starknet_specs_rev.txt.
# Skips cloning if the target directory already contains the required subdirs.
#
# Usage: prepare_starknet_specs.sh <target_dir>
set -euo pipefail

TARGET_DIR="${1:-/tmp/starknet-specs}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
REV=$(tr -d '[:space:]' < "$REPO_ROOT/crates/starknet_transaction_prover/resources/starknet_specs_rev.txt")

if [[ -d "$TARGET_DIR/api" && -d "$TARGET_DIR/proving-api" ]]; then
    exit 0
fi

git clone --filter=blob:none --sparse https://github.com/starkware-libs/starknet-specs.git "$TARGET_DIR"
git -C "$TARGET_DIR" checkout "$REV"
git -C "$TARGET_DIR" sparse-checkout set api proving-api
