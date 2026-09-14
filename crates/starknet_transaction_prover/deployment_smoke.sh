#!/usr/bin/env bash

set -euo pipefail

KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-false}"
LOOKBACK_BLOCKS="${LOOKBACK_BLOCKS:-300}"
PROVE_MAX_TIME="${PROVE_MAX_TIME:-300}"
TLS_MIN_DAYS="${TLS_MIN_DAYS:-30}"
MAX_REQUEST_BODY_SIZE="${MAX_REQUEST_BODY_SIZE:-$((5 * 1024 * 1024))}"

# JSON-RPC error codes asserted below, per the proving-api OpenRPC spec.
readonly RPC_BLOCK_NOT_FOUND=24
readonly RPC_SERVICE_BUSY=-32005
readonly RPC_INVALID_PARAMS=-32602
readonly RPC_INTERNAL_ERROR=-32603

# Seconds. JSON-RPC POSTs get RPC_CALL_MAX_TIME; plain GET probes (/health, /ohttp-keys) get the
# shorter HTTP_PROBE_MAX_TIME. A real starknet_proveTransaction overrides both with
# PROVE_MAX_TIME, since a full SNOS proof can take minutes.
readonly RPC_CALL_MAX_TIME=30
readonly HTTP_PROBE_MAX_TIME=10

readonly NUM_CONCURRENT_REQUESTS=3
readonly SCAN_PROGRESS_INTERVAL=50

# Bash evaluates command substitution inside `$(( ))` and `[[ -ge ]]`, so any value that reaches
# an arithmetic context is executable code unless it is known to be a number. That covers these
# operator-supplied settings and, further down, the block numbers the chain RPC hands back.
is_non_negative_integer() {
    [[ "$1" =~ ^[0-9]+$ ]]
}

for numeric_var_name in LOOKBACK_BLOCKS PROVE_MAX_TIME TLS_MIN_DAYS MAX_REQUEST_BODY_SIZE; do
    if ! is_non_negative_integer "${!numeric_var_name}"; then
        echo "ERROR: $numeric_var_name must be a non-negative integer, got" \
            "'${!numeric_var_name}'." >&2
        exit 2
    fi
done

# Counters must exist before the EXIT trap is installed — the trap reads `$FAIL_COUNT` under
# `set -u`, so a signal arriving between `trap` and these assignments would abort cleanup.
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

# Auto-detect spec version from the Rust source (single source of truth).
# POSIX sed rather than GNU-only `grep -P`, so detection works on macOS/BSD.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPEC_VERSION_EXPECTED=$(sed -n 's/.*const SPEC_VERSION: &str = "\([^"]*\)".*/\1/p' \
    "$SCRIPT_DIR/src/server/rpc_impl.rs" 2>/dev/null || true)
if [[ -z "$SPEC_VERSION_EXPECTED" ]]; then
    # Detection only fails when the script runs away from a source checkout (e.g. copied onto a
    # bastion host) — there is no safe hardcoded fallback since this constant changes across spec
    # revisions, so require an explicit override instead of silently checking against a stale value.
    if [[ -n "${SPEC_VERSION:-}" ]]; then
        SPEC_VERSION_EXPECTED="$SPEC_VERSION"
    else
        echo "ERROR: could not auto-detect SPEC_VERSION from $SCRIPT_DIR/src/server/rpc_impl.rs." >&2
        echo "Run this script from a repo checkout, or export SPEC_VERSION explicitly." >&2
        exit 2
    fi
fi

# Created after the early `exit 2` paths above so those cannot leak the directory.
TMP_DIR="$(mktemp -d)"

cleanup() {
    if [[ "$KEEP_ARTIFACTS" == "true" || "$FAIL_COUNT" -gt 0 ]]; then
        echo "Artifacts preserved in $TMP_DIR"
    else
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

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

# A check that could not run. Reported in the verdict line so a pass is never mistaken for
# coverage the run did not actually have.
skip_step() {
    SKIP_COUNT=$((SKIP_COUNT + 1))
    echo "SKIP: $1"
}

fail_step() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    echo "FAIL: $1"
}

# Strips userinfo, path, and query from a URL, keeping only scheme://host[:port]. Provider RPC
# URLs routinely embed API keys in the path, query string, or basic-auth userinfo
# (e.g. https://host/v3/<api-key>, https://user:token@host), so logging must never echo one
# verbatim.
url_scheme_host() {
    local url="$1"
    local scheme="${url%%://*}"
    local rest="${url#*://}"
    local host="${rest%%/*}"
    host="${host%%\?*}"
    host="${host##*@}"
    echo "${scheme}://${host}"
}

rpc_call_chain() {
    local payload="$1"
    # `|| true`: surfaced as a FAIL by the caller, don't abort under `set -e`.
    curl -sS --max-time "$RPC_CALL_MAX_TIME" "$CHAIN_RPC_URL" -H 'content-type: application/json' -d "$payload" || true
}

rpc_call_prover() {
    local payload="$1"
    local max_time="${2:-$RPC_CALL_MAX_TIME}"
    curl -sS --max-time "$max_time" "$PROVER_URL" -H 'content-type: application/json' -d "$payload" || true
}

# Echoes "<block_number> <tx_hash>" for the newest matching transaction.
find_tx_block_and_hash() {
    local tx_type="$1"
    local tx_version="$2"
    local lookback="$3"
    local latest_block
    local offset
    local block_number
    local tx_hash

    latest_block=$(rpc_call_chain '{"jsonrpc":"2.0","id":100,"method":"starknet_blockNumber","params":[]}' | jq -r '.result' 2>/dev/null || true)
    if ! is_non_negative_integer "$latest_block"; then
        # Distinct exit code so callers can tell "chain RPC unreachable/invalid response" apart
        # from the ordinary "no matching tx in lookback window" miss (exit 1, below), instead of
        # feeding an empty/null value into the arithmetic below or silently scanning from block 0.
        echo "  ERROR: chain RPC returned no usable block number for starknet_blockNumber" >&2
        return 2
    fi
    echo "  Latest block: $latest_block (scanning up to $lookback blocks for $tx_type $tx_version)" >&2

    for ((offset = 0; offset < lookback; offset++)); do
        block_number=$((latest_block - offset))
        [[ "$block_number" -lt 0 ]] && break

        if (( offset % SCAN_PROGRESS_INTERVAL == 0 && offset > 0 )); then
            echo "  Scanned $offset blocks so far (at block $block_number)..." >&2
        fi

        tx_hash=$(rpc_call_chain "{\"jsonrpc\":\"2.0\",\"id\":101,\"method\":\"starknet_getBlockWithTxs\",\"params\":[{\"block_number\":$block_number}]}" \
            | jq -r --arg tx_type "$tx_type" --arg tx_version "$tx_version" \
                '[.result.transactions[] | select(.type==$tx_type and .version==$tx_version) | .transaction_hash] | .[0] // empty')

        if [[ -n "$tx_hash" && "$tx_hash" != "null" ]]; then
            echo "  Found $tx_type $tx_version tx at block $block_number (offset $offset)" >&2
            echo "$block_number $tx_hash"
            return 0
        fi
    done

    return 1
}

check_health() {
    log_step "Check /health endpoint"

    local http_status
    http_status=$(curl -sS -o "$TMP_DIR/health.json" -w '%{http_code}' --max-time "$HTTP_PROBE_MAX_TIME" \
        "$PROVER_URL/health" || echo "000")

    if [[ "$http_status" != "200" ]]; then
        fail_step "/health returned HTTP $http_status (expected 200)"
        return 0
    fi
    if jq -e '.status == "ok"' "$TMP_DIR/health.json" >/dev/null 2>&1; then
        pass_step "/health returned HTTP 200 with body {\"status\":\"ok\"}"
    else
        fail_step "/health returned HTTP 200 but body was not {\"status\":\"ok\"}"
    fi
}

check_spec_version() {
    log_step "Check starknet_specVersion"
    local response
    response=$(rpc_call_prover '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}')
    echo "$response" > "$TMP_DIR/spec_version.json"

    if jq -e --arg expected "$SPEC_VERSION_EXPECTED" '.result == $expected' "$TMP_DIR/spec_version.json" >/dev/null; then
        pass_step "starknet_specVersion returned $SPEC_VERSION_EXPECTED"
    else
        fail_step "starknet_specVersion did not return $SPEC_VERSION_EXPECTED"
    fi
}

check_compression() {
    log_step "Check HTTP response compression"
    # Use `gzip` (and not `zstd`) because curl's `--compressed` only reliably decompresses gzip
    # and deflate; zstd needs a curl built with libzstd, which is not universally available.
    local headers
    headers=$(curl -sS -D- --compressed "$PROVER_URL" \
        -H 'content-type: application/json' \
        -H 'accept-encoding: gzip' \
        -d '{"jsonrpc":"2.0","id":2,"method":"starknet_specVersion","params":[]}' \
        -o "$TMP_DIR/compressed_resp.json" 2>&1 || true)

    if echo "$headers" | grep -qi '^content-encoding:'; then
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
        if ! is_non_negative_integer "$TX_BLOCK"; then
            fail_step "Chain RPC returned no usable block number for tx $TX_HASH"
            return 1
        fi
    else
        local find_result
        local find_status=0
        find_result=$(find_tx_block_and_hash "INVOKE" "0x3" "$LOOKBACK_BLOCKS") || find_status=$?
        if [[ "$find_status" -eq 2 ]]; then
            fail_step "Chain RPC unreachable/invalid response for starknet_blockNumber"
            return 1
        fi
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
    # `del` on a null `.result` yields null and exits 0, so a JSON-RPC error body would reach
    # here with no jq diagnostic.
    if ! jq -e '.result != null' "$TMP_DIR/prove_tx_raw.json" >/dev/null 2>&1; then
        fail_step "Chain RPC returned no transaction object for tx $TX_HASH"
        return 1
    fi
    jq '.result | del(.transaction_hash)' "$TMP_DIR/prove_tx_raw.json" > "$TMP_DIR/prove_tx.json"

    BASE_BLOCK=$((TX_BLOCK - 1))
    if [[ "$BASE_BLOCK" -lt 0 ]]; then
        fail_step "Computed base block is negative for tx $TX_HASH"
        return 1
    fi

    jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx "$TMP_DIR/prove_tx.json" \
        '{jsonrpc:"2.0",id:5,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
        > "$TMP_DIR/prove_request_valid.json"

    pass_step "Built valid prove request using tx_hash=$TX_HASH and base_block=$BASE_BLOCK"
}

check_pending_block_rejected() {
    log_step "Check starknet_proveTransaction rejects a pending block_id"

    # Rejected during up-front validation before the OS runs, so this stays cheap despite
    # depending on a request that needed a real chain lookup.
    local request
    request=$(jq -c '.params[0] = "pending"' "$TMP_DIR/prove_request_valid.json")

    local response
    response=$(rpc_call_prover "$request")
    echo "$response" > "$TMP_DIR/prove_pending.json"

    if jq -e --argjson expected "$RPC_BLOCK_NOT_FOUND" '.error.code == $expected' "$TMP_DIR/prove_pending.json" >/dev/null; then
        pass_step "starknet_proveTransaction rejected pending block_id with error code $RPC_BLOCK_NOT_FOUND (Block not found)"
    else
        fail_step "starknet_proveTransaction did not reject pending block_id with error code $RPC_BLOCK_NOT_FOUND (Block not found)"
    fi
}

check_prove_happy_path() {
    log_step "Check starknet_proveTransaction happy path"

    local response
    response=$(rpc_call_prover "$(cat "$TMP_DIR/prove_request_valid.json")" "$PROVE_MAX_TIME")
    echo "$response" > "$TMP_DIR/prove_happy.json"

    if jq -e '.result.proof and .result.proof_facts' "$TMP_DIR/prove_happy.json" >/dev/null; then
        pass_step "starknet_proveTransaction returned proof and proof_facts"
    else
        fail_step "starknet_proveTransaction happy path failed"
    fi
}

check_malformed_params() {
    log_step "Check malformed params rejection"
    local response
    response=$(rpc_call_prover '{"jsonrpc":"2.0","id":16,"method":"starknet_proveTransaction","params":["latest"]}')
    echo "$response" > "$TMP_DIR/malformed.json"

    # Pin the exact code: a bare `.error != null` also passes on -32601 (method missing) and
    # -32603 (handler blew up), which are the two outcomes a smoke test most wants to catch.
    if jq -e --argjson expected "$RPC_INVALID_PARAMS" '.error.code == $expected' "$TMP_DIR/malformed.json" >/dev/null; then
        pass_step "Malformed params rejected with error code $RPC_INVALID_PARAMS (Invalid params)"
    else
        fail_step "Malformed params did not return error code $RPC_INVALID_PARAMS (got: $(jq -c '.error.code // "no error"' "$TMP_DIR/malformed.json"))"
    fi
}

# `busy_count` is informational: the default `max_concurrent_requests` comfortably serves this
# many requests, so a zero busy_count is not a problem. Forcing a rejection needs a deployment
# with `max_concurrent_requests: 1` — see MANUAL_TESTING_GUIDE.md.
check_concurrency_and_recovery() {
    log_step "Check service handles concurrent load and recovers"

    local request_index
    local pids=()
    local transport_failures=0

    for ((request_index = 1; request_index <= NUM_CONCURRENT_REQUESTS; request_index++)); do
        (
            curl -sS --max-time "$PROVE_MAX_TIME" "$PROVER_URL" -H 'content-type: application/json' \
                -d "$(cat "$TMP_DIR/prove_request_valid.json")" > "$TMP_DIR/concurrency_$request_index.json"
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
    for ((request_index = 1; request_index <= NUM_CONCURRENT_REQUESTS; request_index++)); do
        local response="$TMP_DIR/concurrency_$request_index.json"
        if [[ ! -s "$response" ]]; then
            continue
        fi
        if jq -e '.result != null' "$response" >/dev/null; then
            success_count=$((success_count + 1))
        fi
        if jq -e --argjson busy "$RPC_SERVICE_BUSY" '.error.code == $busy' "$response" >/dev/null; then
            busy_count=$((busy_count + 1))
        fi
        if jq -e --argjson internal "$RPC_INTERNAL_ERROR" '.error.code == $internal' "$response" >/dev/null; then
            internal_error_count=$((internal_error_count + 1))
        fi
    done

    # Every response must be either a proof or an explicit busy rejection. Accepting "at least one
    # success" let the other requests come back with any unrelated error code unnoticed.
    local accounted_for=$((success_count + busy_count))
    if [[ "$accounted_for" -eq "$NUM_CONCURRENT_REQUESTS" && "$transport_failures" -eq 0 ]]; then
        pass_step "Service handled concurrent load (success=$success_count busy=$busy_count [informational])"
    else
        fail_step "Service did not handle concurrent load cleanly (success=$success_count busy=$busy_count [informational] transport_failures=$transport_failures internal_errors=$internal_error_count of $NUM_CONCURRENT_REQUESTS)"
    fi

    local recovery_response
    recovery_response=$(rpc_call_prover '{"jsonrpc":"2.0","id":17,"method":"starknet_specVersion","params":[]}')
    echo "$recovery_response" > "$TMP_DIR/recovery.json"

    if jq -e --arg expected "$SPEC_VERSION_EXPECTED" '.result == $expected' "$TMP_DIR/recovery.json" >/dev/null; then
        pass_step "Service recovery check passed"
    else
        fail_step "Service recovery check failed"
    fi
}

# Fails, not warns, when the certificate expires within TLS_MIN_DAYS days.
check_tls_certificate() {
    if [[ "$PROVER_URL" != https://* ]]; then
        skip_step "TLS cert check — PROVER_URL is not https://"
        return 0
    fi
    if ! command -v openssl >/dev/null 2>&1; then
        skip_step "TLS cert check — openssl not installed on this host"
        return 0
    fi
    log_step "Check TLS certificate validity"

    local host_port
    host_port="${PROVER_URL#https://}"
    host_port="${host_port%%/*}"
    local host="${host_port%%:*}"
    local port="${host_port##*:}"
    [[ "$host" == "$port" ]] && port=443

    # `|| true` keeps `set -euo pipefail` from aborting the whole script when `openssl s_client`
    # can't connect (firewall, timeout) or `openssl x509` gets invalid input — the empty-string
    # check below already surfaces those as a FAIL entry.
    local not_after
    not_after=$(echo | openssl s_client -servername "$host" -connect "$host:$port" 2>/dev/null \
        | openssl x509 -noout -enddate 2>/dev/null | sed -n 's/notAfter=//p' || true)

    if [[ -z "$not_after" ]]; then
        fail_step "TLS cert check could not read certificate from $host:$port"
        return 0
    fi

    # GNU `date -d` first; fall back to BSD/macOS `date -j -f` with the strptime format matching
    # openssl's `-enddate` output (e.g. "Aug  1 04:00:00 2027 GMT"). `%e` accepts the
    # space-padded day openssl emits for single-digit days.
    local expiry_epoch
    expiry_epoch=$(date -d "$not_after" +%s 2>/dev/null) \
        || expiry_epoch=$(date -j -f '%b %e %T %Y %Z' "$not_after" +%s 2>/dev/null) \
        || expiry_epoch=0

    if [[ "$expiry_epoch" -eq 0 ]]; then
        fail_step "TLS cert check could not parse expiry date '$not_after' (tried GNU and BSD date)"
        return 0
    fi

    local now_epoch
    now_epoch=$(date +%s)
    local days_left=$(( (expiry_epoch - now_epoch) / 86400 ))

    if [[ "$days_left" -ge "$TLS_MIN_DAYS" ]]; then
        pass_step "TLS cert valid for $days_left days (≥ $TLS_MIN_DAYS)"
    else
        fail_step "TLS cert expires in $days_left days (< $TLS_MIN_DAYS); notAfter=$not_after"
    fi
}

# OHTTP is optional; most deployments will not run this check.
check_ohttp_keys() {
    if [[ "${OHTTP_SMOKE:-false}" != "true" ]]; then
        skip_step "/ohttp-keys check — set OHTTP_SMOKE=true to enable"
        return 0
    fi
    log_step "Check /ohttp-keys endpoint"

    local http_status
    http_status=$(curl -sS -o "$TMP_DIR/ohttp_keys.bin" -D "$TMP_DIR/ohttp_keys.headers" \
        -w '%{http_code}' --max-time "$HTTP_PROBE_MAX_TIME" "$PROVER_URL/ohttp-keys" || echo "000")

    if [[ "$http_status" != "200" ]]; then
        fail_step "/ohttp-keys returned HTTP $http_status (expected 200)"
        return 0
    fi
    if [[ ! -s "$TMP_DIR/ohttp_keys.bin" ]]; then
        fail_step "/ohttp-keys returned an empty body"
        return 0
    fi
    if ! grep -qi '^cache-control:' "$TMP_DIR/ohttp_keys.headers"; then
        fail_step "/ohttp-keys response missing cache-control header"
        return 0
    fi
    # Anchored to the cache-control line: an unanchored match is satisfied by an
    # ingress-added `strict-transport-security: max-age=...` while cache-control says max-age=0.
    if ! grep -i '^cache-control:' "$TMP_DIR/ohttp_keys.headers" | grep -qi 'max-age=[1-9]'; then
        fail_step "/ohttp-keys cache-control header is missing a non-zero max-age"
        return 0
    fi
    pass_step "/ohttp-keys returns non-empty key material with a cacheable (non-zero max-age) cache-control header"
}

check_body_size_limit() {
    log_step "Check max_request_body_size enforcement"

    local oversize_body_bytes=$((MAX_REQUEST_BODY_SIZE + 1024))

    local http_status
    http_status=$(jq -nc --argjson pad_bytes "$oversize_body_bytes" \
        '{jsonrpc:"2.0",id:1,method:"starknet_specVersion",params:[],pad:("x" * $pad_bytes)}' \
        | curl -sS -o "$TMP_DIR/body_oversize.txt" -w '%{http_code}' --max-time "$RPC_CALL_MAX_TIME" \
            "$PROVER_URL" -H 'content-type: application/json' --data-binary @- || echo "000")

    # A 200 carrying a spec-version result would mean the limit is not enforced.
    local oversize_rejected=false
    if [[ "$http_status" == "413" ]]; then
        oversize_rejected=true
    elif jq -e '.error' "$TMP_DIR/body_oversize.txt" >/dev/null 2>&1; then
        oversize_rejected=true
    fi

    if [[ "$oversize_rejected" != true ]]; then
        fail_step "Oversize body not rejected (http=$http_status, expected 413 or JSON-RPC error)"
        return 0
    fi

    local live_response
    live_response=$(rpc_call_prover '{"jsonrpc":"2.0","id":999,"method":"starknet_specVersion","params":[]}')
    if jq -e --arg expected "$SPEC_VERSION_EXPECTED" '.result == $expected' <<<"$live_response" >/dev/null; then
        pass_step "Oversize body rejected (http=$http_status); service still healthy"
    else
        fail_step "Service unresponsive after oversize body request"
    fi
}

main() {
    require_cmd curl
    require_cmd jq

    require_env PROVER_URL
    require_env CHAIN_RPC_URL

    echo "Running starknet_transaction_prover deployment smoke tests"
    echo "PROVER_URL=$(url_scheme_host "$PROVER_URL")"
    echo "CHAIN_RPC_URL=$(url_scheme_host "$CHAIN_RPC_URL")"
    echo "SPEC_VERSION_EXPECTED=$SPEC_VERSION_EXPECTED"
    echo "LOOKBACK_BLOCKS=$LOOKBACK_BLOCKS"
    echo "KEEP_ARTIFACTS=$KEEP_ARTIFACTS"
    echo "PROVE_MAX_TIME=$PROVE_MAX_TIME"
    echo "MAX_REQUEST_BODY_SIZE=$MAX_REQUEST_BODY_SIZE"
    [[ -n "${TX_HASH:-}" ]] && echo "TX_HASH=$TX_HASH (pre-set, will skip block scan)"

    check_health
    check_spec_version
    check_compression
    check_tls_certificate
    check_ohttp_keys
    check_body_size_limit
    # Independent of build_valid_prove_request: sends a hardcoded payload and uses none of its
    # artifacts, so it runs even when the build step fails.
    check_malformed_params
    if build_valid_prove_request; then
        check_pending_block_rejected
        check_prove_happy_path
        check_concurrency_and_recovery
    fi

    echo ""
    echo "Smoke test summary: PASS=$PASS_COUNT FAIL=$FAIL_COUNT SKIP=$SKIP_COUNT"

    if [[ "$FAIL_COUNT" -eq 0 ]]; then
        if [[ "$SKIP_COUNT" -gt 0 ]]; then
            echo "Overall result: PASS ($SKIP_COUNT check(s) skipped, not validated)"
        else
            echo "Overall result: PASS"
        fi
        exit 0
    fi

    echo "Overall result: FAIL"
    exit 1
}

main "$@"
