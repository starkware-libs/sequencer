#!/usr/bin/env bash

set -euo pipefail

KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-false}"
LOOKBACK_BLOCKS="${LOOKBACK_BLOCKS:-300}"
TMP_DIR="$(mktemp -d)"

# Auto-detect spec version from the Rust source (single source of truth).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPEC_VERSION_EXPECTED=$(grep -oP 'const SPEC_VERSION: &str = "\K[^"]+' \
    "$SCRIPT_DIR/src/server/rpc_impl.rs" 2>/dev/null \
    || echo "0.10.1")

cleanup() {
    if [[ "$KEEP_ARTIFACTS" == "true" || "$FAIL_COUNT" -gt 0 ]]; then
        echo "Artifacts preserved in $TMP_DIR"
    else
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

PASS_COUNT=0
FAIL_COUNT=0

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "ERROR: required command '$cmd' is not installed."
        exit 2
    fi
}

require_env() {
    local name="$1"
    if [[ -z "${!name:-}" ]]; then
        echo "ERROR: environment variable $name must be set."
        exit 2
    fi
}

log_step() {
    echo ""
    echo "==> $1"
}

pass_step() {
    PASS_COUNT=$((PASS_COUNT + 1))
    echo "PASS: $1"
}

fail_step() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    echo "FAIL: $1"
}

rpc_call_chain() {
    local payload="$1"
    curl -sS --max-time 30 "$CHAIN_RPC_URL" -H 'content-type: application/json' -d "$payload"
}

rpc_call_prover() {
    local payload="$1"
    curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$payload"
}

find_tx_hash() {
    local tx_type="$1"
    local tx_version="$2"
    local lookback="$3"
    local latest_block
    local offset
    local block_number
    local tx_hash

    latest_block=$(rpc_call_chain '{"jsonrpc":"2.0","id":100,"method":"starknet_blockNumber","params":[]}' | jq -r '.result')
    echo "  Latest block: $latest_block (scanning up to $lookback blocks for $tx_type $tx_version)" >&2

    for ((offset = 0; offset <= lookback; offset++)); do
        block_number=$((latest_block - offset))
        [[ "$block_number" -lt 0 ]] && break

        if (( offset % 50 == 0 && offset > 0 )); then
            echo "  Scanned $offset blocks so far (at block $block_number)..." >&2
        fi

        tx_hash=$(rpc_call_chain "{\"jsonrpc\":\"2.0\",\"id\":101,\"method\":\"starknet_getBlockWithTxs\",\"params\":[{\"block_number\":$block_number}]}" \
            | jq -r --arg tx_type "$tx_type" --arg tx_version "$tx_version" \
                '.result.transactions[] | select(.type==$tx_type and .version==$tx_version) | .transaction_hash' \
            | head -n 1)

        if [[ -n "$tx_hash" && "$tx_hash" != "null" ]]; then
            echo "  Found $tx_type $tx_version tx at block $block_number (offset $offset)" >&2
            echo "$block_number $tx_hash"
            return 0
        fi
    done

    return 1
}

zero_fee_fields() {
    local input_file="$1"
    local output_file="$2"
    jq '
      .tip = "0x0" |
      .resource_bounds.l1_gas.max_price_per_unit = "0x0" |
      .resource_bounds.l2_gas.max_price_per_unit = "0x0" |
      .resource_bounds.l1_data_gas.max_price_per_unit = "0x0"
    ' "$input_file" > "$output_file"
}

check_spec_version() {
    log_step "Check starknet_specVersion"
    local resp
    resp=$(rpc_call_prover '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}')
    echo "$resp" > "$TMP_DIR/spec_version.json"

    if jq -e --arg expected "$SPEC_VERSION_EXPECTED" '.result == $expected' "$TMP_DIR/spec_version.json" >/dev/null; then
        pass_step "starknet_specVersion returned $SPEC_VERSION_EXPECTED"
    else
        fail_step "starknet_specVersion did not return $SPEC_VERSION_EXPECTED"
    fi
}

check_compression() {
    log_step "Check HTTP response compression"
    local headers
    headers=$(curl -sS -D- --compressed "$PROVER_URL" \
        -H 'content-type: application/json' \
        -H 'accept-encoding: zstd' \
        -d '{"jsonrpc":"2.0","id":2,"method":"starknet_specVersion","params":[]}' \
        -o "$TMP_DIR/compressed_resp.json" 2>&1)

    if echo "$headers" | grep -qi 'content-encoding'; then
        local result
        result=$(jq -r '.result' "$TMP_DIR/compressed_resp.json" 2>/dev/null || true)
        if [[ "$result" == "$SPEC_VERSION_EXPECTED" ]]; then
            pass_step "Compressed response returned valid JSON with correct spec version"
        else
            fail_step "Compressed response did not contain expected spec version (got: $result)"
        fi
    else
        fail_step "No content-encoding header in response (compression layer may not be active)"
    fi
}

build_valid_prove_request() {
    log_step "Build valid starknet_proveTransaction request"

    if [[ -n "${TX_HASH:-}" ]]; then
        echo "Using pre-set TX_HASH=$TX_HASH (skipping block scan)"
        echo "  Fetching tx receipt for block number..."
        TX_BLOCK=$(rpc_call_chain "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"starknet_getTransactionReceipt\",\"params\":[\"$TX_HASH\"]}" \
            | jq -r '.result.block_number')
        if [[ "$TX_BLOCK" == "null" || -z "$TX_BLOCK" ]]; then
            fail_step "Could not resolve transaction block number for tx $TX_HASH"
            return 1
        fi
    else
        local find_result
        find_result=$(find_tx_hash "INVOKE" "0x3" "$LOOKBACK_BLOCKS" || true)
        if [[ -z "$find_result" ]]; then
            fail_step "No INVOKE 0x3 tx found in last $LOOKBACK_BLOCKS blocks"
            return 1
        fi
        TX_BLOCK="${find_result%% *}"
        TX_HASH="${find_result#* }"
    fi

    echo "  Fetching tx object for $TX_HASH..."
    rpc_call_chain "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$TX_HASH\"]}" \
        > "$TMP_DIR/prove_tx_raw.json"
    echo "  Got response ($(wc -c < "$TMP_DIR/prove_tx_raw.json") bytes), extracting tx..."
    jq '.result | del(.transaction_hash)' "$TMP_DIR/prove_tx_raw.json" > "$TMP_DIR/prove_tx.json"

    BASE_BLOCK=$((TX_BLOCK - 1))
    if [[ "$BASE_BLOCK" -lt 0 ]]; then
        fail_step "Computed base block is negative for tx $TX_HASH"
        return 1
    fi

    zero_fee_fields "$TMP_DIR/prove_tx.json" "$TMP_DIR/prove_tx_zeroed.json"

    jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx "$TMP_DIR/prove_tx_zeroed.json" \
        '{jsonrpc:"2.0",id:5,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
        > "$TMP_DIR/prove_request_valid.json"

    pass_step "Built valid prove request using tx_hash=$TX_HASH and base_block=$BASE_BLOCK"
}

check_prove_happy_path() {
    log_step "Check starknet_proveTransaction happy path"

    local resp
    resp=$(rpc_call_prover "$(cat "$TMP_DIR/prove_request_valid.json")")
    echo "$resp" > "$TMP_DIR/prove_happy.json"

    if jq -e '.result.proof and .result.proof_facts' "$TMP_DIR/prove_happy.json" >/dev/null; then
        pass_step "starknet_proveTransaction returned proof and proof_facts"
    else
        fail_step "starknet_proveTransaction happy path failed"
    fi
}

check_malformed_params() {
    log_step "Check malformed params rejection"
    local resp
    resp=$(rpc_call_prover '{"jsonrpc":"2.0","id":16,"method":"starknet_proveTransaction","params":["latest"]}')
    echo "$resp" > "$TMP_DIR/malformed.json"

    if jq -e '.error != null' "$TMP_DIR/malformed.json" >/dev/null; then
        pass_step "Malformed params returned JSON-RPC error"
    else
        fail_step "Malformed params did not return an error"
    fi
}

check_concurrency_and_recovery() {
    log_step "Check concurrency behavior and recovery"

    local i
    local pids=()
    local transport_failures=0

    for i in 1 2 3; do
        (
            curl -sS "$PROVER_URL" -H 'content-type: application/json' \
                -d "$(cat "$TMP_DIR/prove_request_valid.json")" > "$TMP_DIR/concurrency_$i.json"
        ) &
        pids+=("$!")
    done

    for pid in "${pids[@]}"; do
        if ! wait "$pid"; then
            transport_failures=$((transport_failures + 1))
        fi
    done

    local success_count=0
    local busy_count=0
    local internal_error_count=0
    for i in 1 2 3; do
        if [[ ! -s "$TMP_DIR/concurrency_$i.json" ]]; then
            continue
        fi
        if jq -e '.result != null' "$TMP_DIR/concurrency_$i.json" >/dev/null; then
            success_count=$((success_count + 1))
        fi
        if jq -e '.error.code == -32005' "$TMP_DIR/concurrency_$i.json" >/dev/null; then
            busy_count=$((busy_count + 1))
        fi
        if jq -e '.error.code == -32603' "$TMP_DIR/concurrency_$i.json" >/dev/null; then
            internal_error_count=$((internal_error_count + 1))
        fi
    done

    if [[ "$success_count" -ge 1 && "$transport_failures" -eq 0 && "$internal_error_count" -eq 0 ]]; then
        pass_step "Concurrency check ok (success=$success_count busy=$busy_count)"
    else
        fail_step "Concurrency check failed (success=$success_count busy=$busy_count transport_failures=$transport_failures internal_errors=$internal_error_count)"
    fi

    local recovery_resp
    recovery_resp=$(rpc_call_prover '{"jsonrpc":"2.0","id":17,"method":"starknet_specVersion","params":[]}')
    echo "$recovery_resp" > "$TMP_DIR/recovery.json"

    if jq -e --arg expected "$SPEC_VERSION_EXPECTED" '.result == $expected' "$TMP_DIR/recovery.json" >/dev/null; then
        pass_step "Service recovery check passed"
    else
        fail_step "Service recovery check failed"
    fi
}

main() {
    require_cmd curl
    require_cmd jq
    require_env PROVER_URL
    require_env CHAIN_RPC_URL

    echo "Running starknet_transaction_prover deployment smoke tests"
    echo "PROVER_URL=$PROVER_URL"
    echo "CHAIN_RPC_URL=$CHAIN_RPC_URL"
    echo "SPEC_VERSION_EXPECTED=$SPEC_VERSION_EXPECTED"
    echo "LOOKBACK_BLOCKS=$LOOKBACK_BLOCKS"
    echo "KEEP_ARTIFACTS=$KEEP_ARTIFACTS"
    [[ -n "${TX_HASH:-}" ]] && echo "TX_HASH=$TX_HASH (pre-set, will skip block scan)"

    check_spec_version
    check_compression
    if build_valid_prove_request; then
        check_prove_happy_path
        check_malformed_params
        check_concurrency_and_recovery
    fi

    echo ""
    echo "Smoke test summary: PASS=$PASS_COUNT FAIL=$FAIL_COUNT"

    if [[ "$FAIL_COUNT" -eq 0 ]]; then
        echo "Overall result: PASS"
        exit 0
    fi

    echo "Overall result: FAIL"
    exit 1
}

main "$@"
