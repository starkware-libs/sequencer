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

# Reuse the existing checkout only when it is already at the pinned revision.
# Otherwise the directory might have a stale revision from a previous run on a persistent runner.
if [[ -d "$TARGET_DIR/.git" && -d "$TARGET_DIR/api" && -d "$TARGET_DIR/proving-api" ]]; then
    current_rev=$(git -C "$TARGET_DIR" rev-parse HEAD 2>/dev/null || echo "")
    if [[ "$current_rev" == "$REV" ]]; then
        exit 0
    fi
fi

rm -rf "$TARGET_DIR"
git clone --filter=blob:none --sparse https://github.com/starkware-libs/starknet-specs.git "$TARGET_DIR"
git -C "$TARGET_DIR" checkout "$REV"
git -C "$TARGET_DIR" sparse-checkout set api proving-api
