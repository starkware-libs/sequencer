# Starknet OS Runner Proving Service Manual Testing Guide

This guide is for engineers testing a deployed `starknet_os_runner` proving service manually.

For quick post-deployment validation (5-10 min), use
`DEPLOYMENT_SMOKE_TESTING_GUIDE.md` instead. This guide is the full
deep-validation reference.

### Time estimates

| Sections | Scope | Approx. time |
|----------|-------|---------------|
| 1-5 | Core endpoint and negative-flow coverage | 15-30 min |
| 6 | CORS matrix | 10-15 min |
| 7 | Concurrency tests | 10-15 min |
| 8 | Load tests | 20-30 min |

## 1. What Is Deployed

`crates/starknet_os_runner/src/main.rs` starts a JSON-RPC server.
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

  for ((offset = 0; offset <= lookback; offset++)); do
    block_number=$((latest_block - offset))
    [ "$block_number" -lt 0 ] && break

    tx_hash=$(rpc_call \
      "{\"jsonrpc\":\"2.0\",\"id\":101,\"method\":\"starknet_getBlockWithTxs\",\"params\":[{\"block_number\":$block_number}]}" \
      | jq -r --arg tx_type "$tx_type" --arg tx_version "$tx_version" \
          '.result.transactions[] | select(.type==$tx_type and .version==$tx_version) | .transaction_hash' \
      | head -n 1)

    if [ -n "$tx_hash" ] && [ "$tx_hash" != "null" ]; then
      echo "$tx_hash"
      return 0
    fi
  done

  return 1
}
```

## 3. Optional: Start Service Locally

```bash
cargo run -p starknet_os_runner -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id mainnet \
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
  `crates/starknet_os_runner/src/server/rpc_impl.rs` (currently `"0.10.0"`)

### 4.2 `starknet_proveTransaction` (happy path)

`starknet_proveTransaction` expects:

- `params[0]`: base block ID (must not be `"pending"`)
- `params[1]`: transaction object of type `INVOKE` and version `0x3`

Use this flow to build a realistic request from chain data.

1. Find one finalized `INVOKE` `0x3` tx hash.

```bash
TX_HASH=$(find_tx_hash "INVOKE" "0x3" 300)
[ -z "$TX_HASH" ] && { echo "No INVOKE 0x3 tx found in lookback window"; exit 1; }

echo "Using tx_hash=$TX_HASH"
```

2. Fetch the tx object and receipt, then compute base block (`tx_block - 1`).

```bash
curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$TX_HASH\"]}" \
  | jq '.result | del(.transaction_hash)' > /tmp/prove_tx.json

TX_BLOCK=$(curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"starknet_getTransactionReceipt\",\"params\":[\"$TX_HASH\"]}" \
  | jq -r '.result.block_number')

BASE_BLOCK=$((TX_BLOCK - 1))
echo "tx_block=$TX_BLOCK base_block=$BASE_BLOCK"
```

3. Build and call `starknet_proveTransaction`.

```bash
jq -c --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/prove_tx.json \
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
jq -c --slurpfile tx /tmp/prove_tx.json \
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
[ -z "$DECLARE_HASH" ] && { echo "No DECLARE 0x3 tx found in lookback window"; exit 1; }

curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":13,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$DECLARE_HASH\"]}" \
  | jq '.result | del(.transaction_hash)' > /tmp/declare_tx.json

jq -c --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/declare_tx.json \
  '{jsonrpc:"2.0",id:14,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_declare.json

curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$(cat /tmp/prove_request_declare.json)" | jq .
```

Expected error:

- `error.code = 61`
- `error.message = "The transaction version is not supported"`
- `error.data` contains:
  `"Declare transactions are not supported; only Invoke transactions are allowed"`

### 5.3 Unsupported tx type: DEPLOY_ACCOUNT

```bash
DEPLOY_HASH=$(find_tx_hash "DEPLOY_ACCOUNT" "0x3" 500)
[ -z "$DEPLOY_HASH" ] && { echo "No DEPLOY_ACCOUNT 0x3 tx found in lookback window"; exit 1; }

curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":17,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$DEPLOY_HASH\"]}" \
  | jq '.result | del(.transaction_hash)' > /tmp/deploy_account_tx.json

jq -c --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/deploy_account_tx.json \
  '{jsonrpc:"2.0",id:18,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_deploy_account.json

curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d "$(cat /tmp/prove_request_deploy_account.json)" | jq .
```

Expected error:

- `error.code = 61`
- `error.message = "The transaction version is not supported"`
- `error.data` contains:
  `"DeployAccount transactions are not supported; only Invoke transactions are allowed"`

### 5.4 Validation failure on invalid invoke

Mutate a valid invoke tx to break validation (for example set nonce to a clearly wrong value).

```bash
jq '.nonce = "0xdeadbeef"' /tmp/prove_tx.json > /tmp/prove_tx_invalid_nonce.json

jq -c --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/prove_tx_invalid_nonce.json \
  '{jsonrpc:"2.0",id:15,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_invalid_nonce.json

curl -sS "$PROVER_URL" -H 'content-type: application/json' -d "$(cat /tmp/prove_request_invalid_nonce.json)" | jq .
```

Expected error:

- `error.code = 55`
- `error.message = "Account validation failed"`
- `error.data` contains validation details

### 5.5 Invalid params / malformed body

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

Start the service with `--cors-allow-origin '*'` (or restart if already running
with different CORS settings).

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

Start the service with a specific allowed origin:

```bash
cargo run -p starknet_os_runner -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id mainnet \
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
  not the server — the server omits the header and the browser blocks the response).

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

Start the service without any `--cors-allow-origin` flag:

```bash
cargo run -p starknet_os_runner -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id mainnet
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

Verify that default ports are stripped and the normalized origin is what the
server matches against:

```bash
cargo run -p starknet_os_runner -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id mainnet \
  --cors-allow-origin http://localhost:5173
```

Send a request with a trailing-slash origin (browsers never send these, but
verify the server handles it gracefully):

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'origin: http://localhost:5173' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' \
  2>&1 | grep -i 'access-control-allow-origin'
```

Pass criteria:

- `access-control-allow-origin: http://localhost:5173`

Also verify that invalid origin values are rejected at startup:

```bash
cargo run -p starknet_os_runner -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id mainnet \
  --cors-allow-origin 'ftp://example.com' 2>&1
```

Pass criteria:

- Service fails to start with an error mentioning `only http:// and https://`.

## 7. Concurrency Tests

Use the same valid request body from section 4.2:

```bash
cp /tmp/prove_request_valid.json /tmp/prove_request.json
```

### 7.1 Verify concurrency limit rejects excess requests

Start the service with `--max-concurrent-requests 1` so only one proving request
runs at a time:

```bash
cargo run -p starknet_os_runner -- \
  --rpc-url "$CHAIN_RPC_URL" \
  --chain-id mainnet \
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

### 7.2 Verify service recovers after rejection

Immediately after section 7.1 (same service instance), send a single request:

```bash
curl -sS "$PROVER_URL" -H "content-type: application/json" \
  -d "$(cat /tmp/prove_request.json)" | jq .
```

Pass criteria:

- The request succeeds with `result.proof` present.
- This confirms the service did not enter a degraded state after hitting capacity.

### 7.3 Burst test

Restart the service with default concurrency (`--max-concurrent-requests 2`).
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

## 8. Load Tests

The goal of load testing is to verify that the concurrency limit keeps memory
bounded and the service stays healthy under sustained pressure.

### 8.1 Monitor memory during load

In a separate terminal, sample the service's resident memory every 2 seconds:

```bash
PROVER_PID=$(pgrep -f starknet_os_runner)
while true; do
  ps -o pid=,rss= -p "$PROVER_PID" 2>/dev/null \
    | awk '{printf "%s  RSS=%d MB\n", strftime("%H:%M:%S"), $2/1024}'
  sleep 2
done
```

### 8.2 Using `vegeta` (recommended)

```bash
echo "POST $PROVER_URL" \
  | vegeta attack -duration=60s -rate=1 -header "Content-Type: application/json" -body /tmp/prove_request.json \
  | tee /tmp/vegeta.bin \
  | vegeta report

vegeta report -type='hist[0,2s,5s,10s,20s,30s]' /tmp/vegeta.bin
```

Start with low rate (`1 req/s`) because proving is CPU heavy.

### 8.3 Using `hey`

```bash
hey -n 20 -c 2 -m POST -H 'Content-Type: application/json' -D /tmp/prove_request.json "$PROVER_URL"
```

### 8.4 Interpreting results

Distinguish between expected and unexpected errors in the output:

- **`-32005` (Service is busy)**: Expected under load. The concurrency limit is
  working correctly — excess requests are rejected instead of consuming memory.
- **`-32603` (Internal error)**: Unexpected. Investigate service logs for panics
  or resource exhaustion.
- **Transport failures** (connection refused / reset): May indicate the
  `--max-connections` limit was hit, or the service crashed. Check if the process
  is still running.

### 8.5 Recovery check

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

## 9. Test Run Checklist

- `starknet_specVersion` returns current spec version (see `rpc_impl.rs`)
- `starknet_proveTransaction` successful end-to-end call completed
- Pending block negative test returned code `24`
- Unsupported tx type negative test(s) returned code `61`
- Invalid invoke validation test returned code `55`
- CORS wildcard: `access-control-allow-origin: *` present
- CORS allowlist: matching origin echoed, non-matching origin omitted
- CORS preflight: OPTIONS response includes correct `access-control-allow-*` headers
- CORS disabled: no `access-control-allow-origin` header
- CORS startup rejection: invalid origin (e.g. `ftp://`) prevents startup
- Concurrency limit rejects excess requests with code `-32005`
- Service recovers and serves requests after hitting capacity
- Burst test: all responses are valid JSON-RPC (success or `-32005`), no `-32603`
- Memory stays bounded under sustained load (no unbounded RSS growth)
- Load test completed at chosen rate without process instability
