#!/usr/bin/env bash
# scripts/generate_proof_flow_fixtures.sh
#
# Regenerates proof.bin and proof_facts.json for the integration_test_proof_flow test.
#
# Step 1: Runs the virtual OS to produce a CairoPie (stable toolchain, fast).
# Step 2: Proves the CairoPie with stwo (nightly toolchain, ~5-10 minutes).
#
# Usage:
#   ./scripts/generate_proof_flow_fixtures.sh
#
# Requirements:
#   - nightly-2025-07-14 Rust toolchain installed (rustup toolchain install nightly-2025-07-14)

set -e

CAIRO_PIE_PATH="${CAIRO_PIE_PATH:-/tmp/proof_flow_cairo_pie.zip}"

echo "==> Step 1: Generating CairoPie (stable toolchain)..."
CAIRO_PIE_PATH="$CAIRO_PIE_PATH" cargo test \
    -p starknet_os_flow_tests \
    generate_cairo_pie \
    -- --ignored --nocapture

echo "==> Step 2: Proving CairoPie (nightly toolchain, this takes ~5-10 minutes)..."
CAIRO_PIE_PATH="$CAIRO_PIE_PATH" cargo +nightly-2025-07-14 run \
    --features stwo_proving \
    --bin generate_proof_flow_fixtures \
    -p starknet_transaction_prover

echo "Done! Fixtures written to crates/apollo_integration_tests/resources/proof_flow/"
