# Starknet Transaction Prover Manual Testing Guide

This guide is for engineers testing a deployed `starknet_transaction_prover` proving service manually.

For quick post-deployment validation (5-10 min), use
`DEPLOYMENT_SMOKE_TESTING_GUIDE.md` instead. This guide is the full
deep-validation reference.

### Time estimates

| Sections | Scope | Approx. time |
|----------|-------|---------------|
| 1-5 | Core endpoint and negative-flow coverage | 15-30 min |
| 6 | CORS matrix | 10-15 min |
| 7 | HTTP response compression | 5-10 min |
| 8 | Concurrency tests | 10-15 min |
| 9 | Load tests | 20-30 min |

## 1. What Is Deployed

`crates/starknet_transaction_prover/src/main.rs` starts a JSON-RPC server.
The deployed API surface is:

- `starknet_specVersion`
- `starknet_proveTransaction`

## 2. Prerequisites

- A running proving service endpoint, for example `http://127.0.0.1:3000`
- Access to a Starknet RPC node for source transactions (same chain as the prover)
- `curl`
- `jq`
- Optional for load tests: `vegeta` or `hey`

Set environment variables:

```bash
export PROVER_URL="http://127.0.0.1:3000"
export CHAIN_RPC_URL="https://your-starknet-rpc"
```

### Changing service configuration

Several sections require starting the service with specific flags. For a
**deployed** service, edit the prover's JSON config file (mounted as a
Kubernetes ConfigMap) and restart the pod. Key config fields and their CLI
equivalents:

| JSON config field | CLI flag | Type |
|-------------------|----------|------|
| `cors_allow_origin` | `--cors-allow-origin` | array of strings (`["*"]` or `["http://..."]`) |
| `max_concurrent_requests` | `--max-concurrent-requests` | integer |
| `validate_zero_fee_fields` | `--skip-fee-field-validation` (inverted) | boolean |

Example: to allow wildcard CORS and lower concurrency to 1, update the config
file:

```json
{
  "cors_allow_origin": ["*"],
  "max_concurrent_requests": 1
}
```

Then restart the pod (or `kubectl rollout restart`).

For **local** testing, pass CLI flags to `cargo run` (see section 3).

Define helper functions once for the rest of the guide:

```bash
rpc_call() {
  local payload="$1"
  curl -sS "$CHAIN_RPC_URL" -H 'content-type: application/json' -d "$payload"
}

find_tx_hash() {
  local tx_type="$1"
  local tx_version="$2"
  local lookback="${3:-200}"
  local latest_block
  local offset
  local block_number
  local tx_hash

  latest_block=$(rpc_call '{"jsonrpc":"2.0","id":100,"method":"starknet_blockNumber","params":[]}' \
    | jq -r '.result')

  printf 'Searching last %d blocks for %s %s...\n' "$lookback" "$tx_type" "$tx_version" >&2

  for ((offset = 0; offset <= lookback; offset++)); do
    block_number=$((latest_block - offset))
    [ "$block_number" -lt 0 ] && break

    printf '\r  block %d (%d/%d)' "$block_number" "$((offset + 1))" "$lookback" >&2

    tx_hash=$(rpc_call \
      "{\"jsonrpc\":\"2.0\",\"id\":101,\"method\":\"starknet_getBlockWithTxs\",\"params\":[{\"block_number\":$block_number}]}" \
      | jq -r --arg tx_type "$tx_type" --arg tx_version "$tx_version" \
          '[.result.transactions[] | select(.type==$tx_type and .version==$tx_version) | .transaction_hash] | .[0] // empty')

    if [ -n "$tx_hash" ] && [ "$tx_hash" != "null" ]; then
      printf '\r  found in block %d (%d/%d)\n' "$block_number" "$((offset + 1))" "$lookback" >&2
      echo "$tx_hash"
      return 0
    fi
  done

  printf '\r  not found after %d blocks\n' "$offset" >&2
  return 1
}
```

## 3. Optional: Start Service Locally

```bash
cargo run -p starknet_transaction_prover -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id SN_MAIN \
  --ip 0.0.0.0 \
  --port 3000 \
  --max-concurrent-requests 2 \
  --max-connections 10 \
  --cors-allow-origin '*'
```

`--max-concurrent-requests` (default: 2) limits how many proving requests run
simultaneously. Excess requests are rejected with error code `-32005`.

`--max-connections` (default: 10) limits the number of simultaneous JSON-RPC
connections the server accepts.

`--cors-allow-origin` configures CORS (Cross-Origin Resource Sharing) for
browser-based clients. Accepts `*` (allow any origin) or one or more specific
origins. Repeat the flag for multiple origins:

```bash
--cors-allow-origin http://localhost:5173 \
--cors-allow-origin https://app.example.com
```

Omit the flag to disable CORS entirely (default). Use `--no-cors` to explicitly
clear origins set in a config file.

The service logs:

- `JSON-RPC proving server is running.` on successful startup, including
  `cors_mode` (`disabled`, `wildcard`, or `allowlist`) and the resolved
  `cors_allow_origin` list.

## 4. Endpoint Coverage

### 4.1 `starknet_specVersion`

Request:

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' | jq .
```

Expected success:

- `result` matches the `SPEC_VERSION` constant in
  `crates/starknet_transaction_prover/src/server/rpc_impl.rs` (currently `"0.10.1"`)

### 4.2 `starknet_proveTransaction` (happy path)

`starknet_proveTransaction` expects:

- `params[0]`: base block ID (must not be `"pending"`)
- `params[1]`: transaction object of type `INVOKE` and version `0x3`

When the prover config has `"validate_zero_fee_fields": false` (or is started
with `--skip-fee-field-validation`), real chain transactions can be sent with
their original fee fields intact. This preserves
the transaction hash and signature, so `__validate__` passes without needing
a dummy-validate account.

1. Find one finalized `INVOKE` `0x3` tx hash and fetch it.

```bash
TX_HASH=$(find_tx_hash "INVOKE" "0x3" 300)
if [ -z "$TX_HASH" ]; then
  echo "No INVOKE 0x3 tx found in lookback window."
  echo "Try a larger lookback: find_tx_hash \"INVOKE\" \"0x3\" 500"
  echo "Sections 4.2-9 require a valid tx -- cannot continue."
else
  curl -sS "$CHAIN_RPC_URL" \
    -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$TX_HASH\"]}" \
    | jq '.result | del(.transaction_hash)' > /tmp/prove_tx.json

  TX_BLOCK=$(curl -sS "$CHAIN_RPC_URL" \
    -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"starknet_getTransactionReceipt\",\"params\":[\"$TX_HASH\"]}" \
    | jq -r '.result.block_number')

  BASE_BLOCK=$((TX_BLOCK - 1))
fi
```

2. Send the prove request.

```bash
jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/prove_tx.json \
  '{jsonrpc:"2.0",id:5,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_valid.json

curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d "$(cat /tmp/prove_request_valid.json)" | jq .
```

Expected success:

- `result.proof` exists
- `result.proof_facts` exists
- `result.l2_to_l1_messages` exists (can be empty)

## 5. Negative Flows (Expected Failures)

Use a valid `INVOKE v3` tx in `/tmp/prove_tx.json` from section 4.2 unless noted.

### 5.1 Pending block is rejected

```bash
jq -nc --slurpfile tx /tmp/prove_tx.json \
  '{jsonrpc:"2.0",id:11,method:"starknet_proveTransaction",params:["pending",$tx[0]]}' \
  > /tmp/prove_request_pending.json

curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$(cat /tmp/prove_request_pending.json)" | jq .
```

Expected error:

- `error.code = 24`
- `error.message = "Block not found"`

### 5.2 Unsupported tx type: DECLARE

Find a `DECLARE` `0x3` tx and drop `transaction_hash` before sending.

```bash
DECLARE_HASH=$(find_tx_hash "DECLARE" "0x3" 500)
if [ -z "$DECLARE_HASH" ]; then
  echo "No DECLARE 0x3 tx found in lookback window -- skipping"
else
  curl -sS "$CHAIN_RPC_URL" \
    -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":13,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$DECLARE_HASH\"]}" \
    | jq '.result | del(.transaction_hash)' > /tmp/declare_tx.json

  jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/declare_tx.json \
    '{jsonrpc:"2.0",id:14,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
    > /tmp/prove_request_declare.json

  curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$(cat /tmp/prove_request_declare.json)" | jq .
fi
```

Expected error:

- `error.code = 61`
- `error.message = "The transaction version is not supported"`

### 5.3 Unsupported tx type: DEPLOY_ACCOUNT

```bash
DEPLOY_HASH=$(find_tx_hash "DEPLOY_ACCOUNT" "0x3" 500)
if [ -z "$DEPLOY_HASH" ]; then
  echo "No DEPLOY_ACCOUNT 0x3 tx found in lookback window -- skipping"
else
  curl -sS "$CHAIN_RPC_URL" \
    -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":17,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$DEPLOY_HASH\"]}" \
    | jq '.result | del(.transaction_hash)' > /tmp/deploy_account_tx.json

  jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/deploy_account_tx.json \
    '{jsonrpc:"2.0",id:18,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
    > /tmp/prove_request_deploy_account.json

  curl -sS "$PROVER_URL" \
    -H 'content-type: application/json' \
    -d "$(cat /tmp/prove_request_deploy_account.json)" | jq .
fi
```

Expected error:

- `error.code = 61`
- `error.message = "The transaction version is not supported"`

### 5.4 Invalid transaction input: non-zero fee fields

> **Note:** This test only applies when `validate_zero_fee_fields` is `true`
> (default). If it is `false` in the config or `--skip-fee-field-validation` is
> passed, non-zero fee fields are accepted and this test should be skipped.

The service rejects transactions with non-zero `max_price_per_unit` or `tip`
(unless `--skip-fee-field-validation` is set). Mutate a valid invoke to trigger
the check.

```bash
jq '.tip = "0x1"' /tmp/prove_tx.json > /tmp/prove_tx_nonzero_tip.json

jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/prove_tx_nonzero_tip.json \
  '{jsonrpc:"2.0",id:19,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_nonzero_tip.json

curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$(cat /tmp/prove_request_nonzero_tip.json)" | jq .
```

Expected error:

- `error.code = 1000`
- `error.message = "Invalid transaction input"`

### 5.5 Validation failure on invalid invoke

Mutate a valid invoke tx to break validation (for example set nonce to a clearly wrong value).

```bash
jq '.nonce = "0xdeadbeef"' /tmp/prove_tx.json > /tmp/prove_tx_invalid_nonce.json

jq -nc --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/prove_tx_invalid_nonce.json \
  '{jsonrpc:"2.0",id:15,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_invalid_nonce.json

curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$(cat /tmp/prove_request_invalid_nonce.json)" | jq .
```

Expected error:

- `error.code = 55`
- `error.message = "Account validation failed"`
- `error.data` contains validation details

### 5.6 Invalid params / malformed body

Wrong method params shape:

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":16,"method":"starknet_proveTransaction","params":["latest"]}' | jq .
```

Expected error:

- JSON-RPC invalid params (`-32602`) or parse/invalid request style error from jsonrpsee

## 6. CORS Tests

CORS controls whether browsers allow web pages to call the proving service.
These tests verify the `Access-Control-*` headers on HTTP responses.

Use a lightweight request (`starknet_specVersion`) for all CORS tests.

### 6.1 Wildcard mode allows any origin

Set `"cors_allow_origin": ["*"]` in the prover config file and restart the pod
(or start locally with `--cors-allow-origin '*'`).

Send a request with an `Origin` header and check for the CORS response header:

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'origin: http://anything.example.com' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1 | grep -i 'access-control-allow-origin'
```

Pass criteria:

- Response contains `access-control-allow-origin: *`.

### 6.2 Allowlist mode returns the matching origin

Set `"cors_allow_origin": ["http://localhost:5173"]` in the prover config file
and restart the pod. For local testing:

```bash
cargo run -p starknet_transaction_prover -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id SN_MAIN \
  --cors-allow-origin http://localhost:5173
```

**Matching origin:**

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:5173' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1 | grep -i 'access-control-allow-origin'
```

Pass criteria:

- Response contains `access-control-allow-origin: http://localhost:5173`.

**Non-matching origin:**

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'origin: http://evil.example.com' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1 | grep -i 'access-control-allow-origin'
```

Pass criteria:

- No `access-control-allow-origin` header in the response.
- The JSON-RPC response body is still returned (CORS is enforced by the browser,
  not the server -- the server omits the header and the browser blocks the response).

### 6.3 Preflight (OPTIONS) request

Browsers send a preflight `OPTIONS` request before cross-origin `POST` requests.
Verify the service responds correctly:

```bash
curl -sS -D- -X OPTIONS "$PROVER_URL" \
  -H 'origin: http://localhost:5173' \
  -H 'access-control-request-method: POST' \
  -H 'access-control-request-headers: content-type' \
  2>&1 | grep -i 'access-control'
```

Pass criteria (with the matching origin from 6.2):

- `access-control-allow-origin: http://localhost:5173`
- `access-control-allow-methods` includes `POST`
- `access-control-allow-headers` includes `content-type`

### 6.4 CORS disabled (default)

Set `"cors_allow_origin": []` in the prover config file and restart the pod.
For local testing, omit the `--cors-allow-origin` flag:

```bash
cargo run -p starknet_transaction_prover -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id SN_MAIN
```

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:5173' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1 | grep -i 'access-control-allow-origin'
```

Pass criteria:

- No `access-control-allow-origin` header in the response.
- Startup log shows `cors_mode=disabled`.

### 6.5 Origin normalization

Verify that invalid origin values are rejected at startup. Set
`"cors_allow_origin": ["ftp://example.com"]` in the config file and restart (or
pass the flag locally):

```bash
cargo run -p starknet_transaction_prover -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id SN_MAIN \
  --cors-allow-origin 'ftp://example.com' 2>&1
```

Pass criteria:

- Service fails to start with an error mentioning `only http:// and https://`.

## 7. HTTP Response Compression

The server applies `CompressionLayer` (from `tower-http`) to all HTTP responses,
supporting gzip, brotli, and zstd. These tests verify compression negotiation
works correctly.

### 7.1 Gzip response

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'accept-encoding: gzip' \
  --compressed \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1
```

Pass criteria:

- Response headers include `content-encoding: gzip`.
- Body decodes to valid JSON with the expected `result` value.

### 7.2 Brotli response

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'accept-encoding: br' \
  --compressed \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1
```

Pass criteria:

- Response headers include `content-encoding: br`.
- Body decodes to valid JSON.

Note: `curl --compressed` handles brotli if curl was built with brotli support.
If your curl build lacks brotli, pipe through `brotli -d` instead.

### 7.3 Zstd response

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'accept-encoding: zstd' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  --output - | zstd -d | jq .
```

Pass criteria:

- Piping through `zstd -d` produces valid JSON with the expected `result` value.

### 7.4 No compression (default)

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1 | grep -i 'content-encoding'
```

Pass criteria:

- No `content-encoding` header in the response.
- Response body is uncompressed valid JSON.

### 7.5 Compressed prove response

> **Note:** If the block from section 4.2 has aged out of the proof window
> (you get error code 42 or -32603 mentioning storage proofs), re-run
> section 4.2 to refresh the request with a recent block.

Proof responses are large and benefit most from compression. Use the valid
request from section 4.2:

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'accept-encoding: gzip' \
  --compressed \
  -d "$(cat /tmp/prove_request_valid.json)" | jq '.result | has("proof")'
```

Pass criteria:

- Response decompresses to valid JSON.
- `result.proof` exists.

## 8. Concurrency Tests

Use the same valid request body from section 4.2:

```bash
cp /tmp/prove_request_valid.json /tmp/prove_request.json
```

> **Note:** Same freshness caveat as section 7.5 — re-run section 4.2 if
> needed.

### 8.1 Verify concurrency limit rejects excess requests

Set `"max_concurrent_requests": 1` in the prover config file and restart the
pod so only one proving request runs at a time. For local testing:

```bash
cargo run -p starknet_transaction_prover -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id SN_MAIN \
  --ip 0.0.0.0 \
  --port 3000 \
  --max-concurrent-requests 1
```

Fire 3 simultaneous requests:

```bash
for i in 1 2 3; do
  curl -sS "$PROVER_URL" -H "content-type: application/json" \
    -d "$(cat /tmp/prove_request.json)" \
    | jq -c "{request:$i, error:.error, has_result:(.result!=null)}" &
done
wait
```

Pass criteria:

- Exactly 1 request succeeds (`has_result: true`).
- The remaining requests return `error.code = -32005` with
  `error.message = "Service is busy"` and `error.data` containing
  `"The proving service is at capacity (1 concurrent request(s)). Please retry later."`.

### 8.2 Verify service recovers after rejection

Immediately after section 8.1 (same service instance), send a single request:

```bash
curl -sS "$PROVER_URL" -H "content-type: application/json" \
  -d "$(cat /tmp/prove_request.json)" | jq .
```

Pass criteria:

- The request succeeds with `result.proof` present.
- This confirms the service did not enter a degraded state after hitting capacity.

### 8.3 Burst test

Reset `"max_concurrent_requests"` to `2` in the config file and restart the pod
(or restart locally with `--max-concurrent-requests 2`).
Fire 8 requests with shell concurrency 4:

```bash
seq 1 8 | xargs -I{} -P4 sh -c '
  curl -sS "$PROVER_URL" -H "content-type: application/json" -d "$(cat /tmp/prove_request.json)" \
  | jq -c "if .error then {id:{},error_code:.error.code} else {id:{},ok:true} end"'
```

Pass criteria:

- Server stays up throughout.
- No transport-level failures (`curl` connection errors).
- Every response is a valid JSON-RPC object.
- Each response is either a success or a `-32005` busy rejection.
- No `-32603` internal errors (these indicate bugs, not load).

## 9. Load Tests

The goal of load testing is to verify that the concurrency limit keeps memory
bounded and the service stays healthy under sustained pressure.

### 9.1 Monitor memory during load

In a separate terminal, sample the service's memory every 2 seconds.

**Kubernetes deployment:**

```bash
watch -n 2 kubectl top pod <pod-name> -n <namespace>
```

**Local process:**

```bash
PROVER_PID=$(pgrep -f starknet_transaction_prover)
while true; do
  ps -o pid=,rss= -p "$PROVER_PID" 2>/dev/null \
    | awk '{printf "%s  RSS=%d MB\n", strftime("%H:%M:%S"), $2/1024}'
  sleep 2
done
```

### 9.2 Using `vegeta` (recommended)

Install ([releases](https://github.com/tsenart/vegeta/releases)):

```bash
curl -sL https://github.com/tsenart/vegeta/releases/download/v12.12.0/vegeta_12.12.0_linux_amd64.tar.gz \
  | tar xz -C /usr/local/bin vegeta
```

Usage:

```bash
echo "POST $PROVER_URL" \
  | vegeta attack -duration=60s -rate=1 -header "Content-Type: application/json" -body /tmp/prove_request.json \
  | tee /tmp/vegeta.bin \
  | vegeta report

vegeta report -type='hist[0,2s,5s,10s,20s,30s]' /tmp/vegeta.bin
```

Start with low rate (`1 req/s`) because proving is CPU heavy.

### 9.3 Using `hey`

Install ([releases](https://github.com/rakyll/hey/releases)):

```bash
curl -sL https://hey-release.s3.us-east-2.amazonaws.com/hey_linux_amd64 -o /usr/local/bin/hey \
  && chmod +x /usr/local/bin/hey
```

Usage:

```bash
hey -n 20 -c 2 -m POST -H 'Content-Type: application/json' -D /tmp/prove_request.json "$PROVER_URL"
```

### 9.4 Interpreting results

Distinguish between expected and unexpected errors in the output:

- **`-32005` (Service is busy)**: Expected under load. The concurrency limit is
  working correctly -- excess requests are rejected instead of consuming memory.
- **`-32603` (Internal error)**: Unexpected. Investigate service logs for panics
  or resource exhaustion.
- **Transport failures** (connection refused / reset): May indicate the
  `--max-connections` limit was hit, or the service crashed. Check if the process
  is still running.

### 9.5 Recovery check

After the load run completes, send a single request to confirm the service is
still healthy:

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' | jq .
```

Pass criteria:

- Memory (RSS) stays bounded and does not grow unboundedly during the run.
- The service process remains alive throughout.
- Error responses are JSON-RPC-formatted (`-32005` or valid results).
- No `-32603` internal errors.
- The recovery check returns the current spec version confirming the service is still responsive.

## 10. Test Run Checklist

- `starknet_specVersion` returns current spec version (see `rpc_impl.rs`)
- `starknet_proveTransaction` successful end-to-end call completed
- Pending block negative test returned code `24`
- Unsupported tx type negative test(s) returned code `61`
- Non-zero fee fields negative test returned code `1000`
- Invalid invoke validation test returned code `55`
- CORS wildcard: `access-control-allow-origin: *` present
- CORS allowlist: matching origin echoed, non-matching origin omitted
- CORS preflight: OPTIONS response includes correct `access-control-allow-*` headers
- CORS disabled: no `access-control-allow-origin` header
- CORS startup rejection: invalid origin (e.g. `ftp://`) prevents startup
- Compression: gzip response includes `content-encoding: gzip` and decodes to valid JSON
- Compression: brotli response includes `content-encoding: br`
- Compression: zstd response decodes via `zstd -d` to valid JSON
- Compression: no `content-encoding` header when `Accept-Encoding` is absent
- Compression: prove response with `--compressed` returns valid `result.proof`
- Concurrency limit rejects excess requests with code `-32005`
- Service recovers and serves requests after hitting capacity
- Burst test: all responses are valid JSON-RPC (success or `-32005`), no `-32603`
- Memory stays bounded under sustained load (no unbounded RSS growth)
- Load test completed at chosen rate without process instability

## 11. Documenting Test Results

Record results after each test run so regressions are traceable and audit
history is preserved.

### What to record

- **Date and tester** — who ran the tests and when.
- **Build version** — the exact image tag, commit SHA, or release version under
  test.
- **Environment** — deployment target (e.g., `testnet-prover-01`, `staging`),
  chain (mainnet / sepolia), and RPC node used.
- **Configuration** — relevant config values: `validate_zero_fee_fields`,
  `max_concurrent_requests`, `cors_allow_origin`.
- **Per-section results** — for each item in the checklist (section 10), record
  PASS / FAIL / SKIP with the actual response when it deviates from expected.
- **Bugs found** — link to any issues filed as a result of the test run.

### Template

Copy the block below and fill in the details:

```text
# Manual Test Run — <date>

Tester:       <name>
Build:        <image tag or commit SHA>
Environment:  <deployment target>
Chain RPC:    <RPC endpoint used>
Config:       validate_zero_fee_fields=<true/false>
              max_concurrent_requests=<value>
              cors_allow_origin=<value or []>

## Results

| # | Check | Result | Notes |
|---|-------|--------|-------|
| 4.1 | specVersion | PASS / FAIL | |
| 4.2 | proveTransaction happy path | PASS / FAIL | proving time: ___s |
| 5.1 | Pending block rejected (24) | PASS / FAIL | |
| 5.2 | DECLARE rejected (61) | PASS / FAIL / SKIP | |
| 5.3 | DEPLOY_ACCOUNT rejected (61) | PASS / FAIL / SKIP | |
| 5.4 | Non-zero fee rejected (1000) | PASS / FAIL / SKIP | |
| 5.5 | Invalid nonce rejected (55) | PASS / FAIL | |
| 5.6 | Malformed params rejected | PASS / FAIL | |
| 6.1 | CORS wildcard | PASS / FAIL | |
| 6.2 | CORS allowlist match | PASS / FAIL | |
| 6.2 | CORS allowlist non-match | PASS / FAIL | |
| 6.3 | CORS preflight | PASS / FAIL | |
| 6.4 | CORS disabled | PASS / FAIL | |
| 6.5 | CORS invalid origin startup | PASS / FAIL | |
| 7.1 | Gzip compression | PASS / FAIL | |
| 7.2 | Brotli compression | PASS / FAIL | |
| 7.3 | Zstd compression | PASS / FAIL | |
| 7.4 | No compression | PASS / FAIL | |
| 7.5 | Compressed prove response | PASS / FAIL | |
| 8.1 | Concurrency limit (-32005) | PASS / FAIL | |
| 8.2 | Recovery after rejection | PASS / FAIL | |
| 8.3 | Burst test | PASS / FAIL | |
| 9   | Load test | PASS / FAIL | tool: ___, rate: ___, duration: ___ |

## Issues Filed

- (link to any issues opened during this run)
```

### Where to store

Post the completed template as a comment on the deployment PR or ticket. For
periodic (non-deployment) test runs, file under the team's test log.
