#!/usr/bin/env bash
# Verifies that the proving-related spec files in the sequencer are in sync with
# the canonical versions in the starknet-specs repository.
#
# Usage:
#   scripts/check_proving_spec_sync.sh [--branch <branch>]
#
# The starknet-specs branch defaults to "main" and can be overridden with --branch.

set -euo pipefail

SPECS_REPO="https://github.com/starkware-libs/starknet-specs.git"
SPECS_BRANCH="main"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --branch) SPECS_BRANCH="$2"; shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

PROVER_RESOURCES="crates/starknet_transaction_prover/resources"

# Files to compare: local path relative to prover resources -> path in starknet-specs repo.
declare -A SPEC_FILES=(
    ["proving_api_openrpc.json"]="api/proving_api_openrpc.json"
    ["starknet_api_openrpc.json"]="api/starknet_api_openrpc.json"
    ["starknet_write_api.json"]="api/starknet_write_api.json"
)

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

echo "Fetching starknet-specs (branch: ${SPECS_BRANCH})..."
git clone --depth 1 --branch "${SPECS_BRANCH}" --filter=blob:none --sparse \
    "${SPECS_REPO}" "${tmpdir}/starknet-specs" 2>/dev/null
(cd "${tmpdir}/starknet-specs" && git sparse-checkout set api)

mismatches=0
for local_file in "${!SPEC_FILES[@]}"; do
    canonical="${tmpdir}/starknet-specs/${SPEC_FILES[$local_file]}"
    local_path="${PROVER_RESOURCES}/${local_file}"

    if [[ ! -f "$local_path" ]]; then
        echo "MISSING: ${local_path} does not exist locally."
        mismatches=$((mismatches + 1))
        continue
    fi

    if ! diff -q "$local_path" "$canonical" > /dev/null 2>&1; then
        echo "OUT OF SYNC: ${local_path}"
        echo "  Run: cp <starknet-specs>/${SPEC_FILES[$local_file]} ${local_path}"
        diff --unified=3 "$local_path" "$canonical" | head -30 || true
        echo ""
        mismatches=$((mismatches + 1))
    else
        echo "OK: ${local_file}"
    fi
done

if [[ $mismatches -gt 0 ]]; then
    echo ""
    echo "${mismatches} file(s) out of sync with starknet-specs (branch: ${SPECS_BRANCH})."
    echo "Copy the updated files from the starknet-specs repo into ${PROVER_RESOURCES}/."
    exit 1
fi

echo "All proving spec files are in sync with starknet-specs."
