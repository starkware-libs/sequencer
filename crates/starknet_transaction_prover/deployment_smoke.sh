#!/usr/bin/env bash

set -euo pipefail

KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-false}"
DUMMY_ACCOUNT_ADDRESS="${DUMMY_ACCOUNT_ADDRESS:-0x2763d2701f413cf0ad7bc73690297c4594bbfd4632ee5f017eb287051595672}"
STRK_TOKEN_ADDRESS="${STRK_TOKEN_ADDRESS:-0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d}"
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

build_prove_request() {
    log_step "Build starknet_proveTransaction request"

    echo "  Fetching nonce for dummy account..."
    local nonce
    nonce=$(rpc_call_chain "{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"starknet_getNonce\",\"params\":[\"latest\",\"$DUMMY_ACCOUNT_ADDRESS\"]}" \
        | jq -r '.result')

    if [[ -z "$nonce" || "$nonce" == "null" ]]; then
        fail_step "Could not fetch nonce for dummy account $DUMMY_ACCOUNT_ADDRESS"
        return 1
    fi
    echo "  Nonce: $nonce"

    jq -nc \
      --arg sender "$DUMMY_ACCOUNT_ADDRESS" \
      --arg strk "$STRK_TOKEN_ADDRESS" \
      --arg nonce "$nonce" \
      '{
        jsonrpc: "2.0",
        id: 5,
        method: "starknet_proveTransaction",
        params: {
          block_id: "latest",
          transaction: {
            type: "INVOKE",
            version: "0x3",
            sender_address: $sender,
            calldata: [$strk, "0x35a73cd311a05d46deda634c5ee045db92f811b4e74bca4437fcb5302b7af33", "0x1", $sender],
            signature: [],
            nonce: $nonce,
            resource_bounds: {
              l1_gas: {max_amount: "0x0", max_price_per_unit: "0x0"},
              l2_gas: {max_amount: "0x5f5e100", max_price_per_unit: "0x0"},
              l1_data_gas: {max_amount: "0x0", max_price_per_unit: "0x0"}
            },
            tip: "0x0",
            paymaster_data: [],
            account_deployment_data: [],
            nonce_data_availability_mode: "L1",
            fee_data_availability_mode: "L1"
          }
        }
      }' > "$TMP_DIR/prove_request_valid.json"

    pass_step "Built prove request for dummy account $DUMMY_ACCOUNT_ADDRESS"
}

check_prove_happy_path() {
    log_step "Check starknet_proveTransaction happy path (may take 30-60s)"

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
    echo "DUMMY_ACCOUNT_ADDRESS=$DUMMY_ACCOUNT_ADDRESS"
    echo "STRK_TOKEN_ADDRESS=$STRK_TOKEN_ADDRESS"
    echo "KEEP_ARTIFACTS=$KEEP_ARTIFACTS"

    check_spec_version
    check_compression
    if build_prove_request; then
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
