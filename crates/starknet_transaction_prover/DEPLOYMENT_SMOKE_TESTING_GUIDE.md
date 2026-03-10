# Starknet Transaction Prover Deployment Smoke Testing Guide

This guide is the short checklist to run after each production deployment of
`starknet_transaction_prover`.

Target runtime: 5-10 minutes.

Use `MANUAL_TESTING_GUIDE.md` for deep validation (CORS matrix, load testing,
and wider negative coverage).

## 1. Scope

This smoke plan validates that:

- The service is reachable.
- Core JSON-RPC methods respond correctly.
- One real proving flow succeeds end to end.
- Invalid requests fail with expected JSON-RPC errors.
- Concurrency protection (`-32005`) works and the service recovers.

Not included in per-deploy smoke:

- Full CORS matrix.
- Sustained load and memory profiling.

Run those periodically (daily/weekly) or before major releases.

## 2. Prerequisites

- A deployed proving service endpoint (for example `http://127.0.0.1:3000`).
- Access to a Starknet RPC node on the same chain as the prover.
- `curl`
- `jq`

Set env vars:

```bash
export PROVER_URL="http://127.0.0.1:3000"
export CHAIN_RPC_URL="https://your-starknet-rpc"
```

One-command option (recommended):

```bash
crates/starknet_transaction_prover/deployment_smoke.sh
```

The script runs Section 3 checks and prints a PASS/FAIL summary.

Optional environment variables:

- `TX_HASH` -- pre-set an `INVOKE` `0x3` tx hash to skip the block scan
  (useful on rate-limited RPCs).
- `LOOKBACK_BLOCKS` -- number of recent blocks to scan (default: 300).
- `KEEP_ARTIFACTS=true` -- preserve temp files for post-mortem inspection.
  Artifacts are also preserved automatically when any check fails.

Define helpers once:

```bash
rpc_call() {
  local payload="$1"
  curl -sS "$CHAIN_RPC_URL" -H 'content-type: application/json' -d "$payload"
}

find_tx_hash() {
  local tx_type="$1"
  local tx_version="$2"
  local lookback="${3:-300}"
  local latest_block
  local offset
  local block_number
  local tx_hash

  latest_block=$(rpc_call '{"jsonrpc":"2.0","id":100,"method":"starknet_blockNumber","params":[]}' | jq -r '.result')

  for ((offset = 0; offset <= lookback; offset++)); do
    block_number=$((latest_block - offset))
    [ "$block_number" -lt 0 ] && break

    tx_hash=$(rpc_call "{\"jsonrpc\":\"2.0\",\"id\":101,\"method\":\"starknet_getBlockWithTxs\",\"params\":[{\"block_number\":$block_number}]}" \
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

## 3. Per-Deployment Smoke Checks

If you use the script above, you can treat this section as reference for what it
executes.

### 3.1 Health check: `starknet_specVersion`

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}' | jq .
```

Pass:

- `result` matches the `SPEC_VERSION` constant in
  `crates/starknet_transaction_prover/src/server/rpc_impl.rs` (currently `"0.10.0"`).

### 3.2 Happy path: one real `starknet_proveTransaction`

1. Find a finalized `INVOKE` `0x3` tx, fetch it, and zero out fee fields.

```bash
TX_HASH=$(find_tx_hash "INVOKE" "0x3" 300)
[ -z "$TX_HASH" ] && { echo "No INVOKE 0x3 tx found in lookback window"; exit 1; }

curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"starknet_getTransactionByHash\",\"params\":[\"$TX_HASH\"]}" \
  | jq '.result | del(.transaction_hash)' > /tmp/prove_tx.json

TX_BLOCK=$(curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"starknet_getTransactionReceipt\",\"params\":[\"$TX_HASH\"]}" \
  | jq -r '.result.block_number')

BASE_BLOCK=$((TX_BLOCK - 1))

jq '
  .tip = "0x0" |
  .resource_bounds.l1_gas.max_price_per_unit = "0x0" |
  .resource_bounds.l2_gas.max_price_per_unit = "0x0" |
  .resource_bounds.l1_data_gas.max_price_per_unit = "0x0"
' /tmp/prove_tx.json > /tmp/prove_tx_zeroed.json
```

2. Send prove request.

```bash
jq -c --argjson base "$BASE_BLOCK" --slurpfile tx /tmp/prove_tx_zeroed.json \
  '{jsonrpc:"2.0",id:5,method:"starknet_proveTransaction",params:[{block_number:$base},$tx[0]]}' \
  > /tmp/prove_request_valid.json

curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d "$(cat /tmp/prove_request_valid.json)" | jq .
```

Pass:

- `result.proof` exists.
- `result.proof_facts` exists.

### 3.3 Negative check: malformed params

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":16,"method":"starknet_proveTransaction","params":["latest"]}' | jq .
```

Pass:

- Error is returned (`-32602` invalid params, or another JSON-RPC parse/shape error).

### 3.4 Concurrency/recovery quick check

Use the valid prove request from 3.2.

```bash
for i in 1 2 3; do
  curl -sS "$PROVER_URL" -H 'content-type: application/json' \
    -d "$(cat /tmp/prove_request_valid.json)" \
    | jq -c "{request:$i, error:.error, ok:(.result!=null)}" &
done
wait
```

Pass:

- At least one request succeeds.
- Excess parallel requests may return `error.code = -32005` (`Service is busy`).
- No transport errors and no `-32603` internal errors.

Then verify immediate recovery:

```bash
curl -sS "$PROVER_URL" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":17,"method":"starknet_specVersion","params":[]}' | jq .
```

Pass:

- Service responds normally (returns the current spec version).

## 4. Pass/Fail Gate

Deployment is smoke-validated only if all checks in Section 3 pass.

If any check fails:

- Capture response JSON and relevant service logs.
- Roll back or hold traffic shift according to deployment policy.
- Run deeper diagnostics from `MANUAL_TESTING_GUIDE.md`.

## 5. Recommended Cadence for Full Tests

- Per deployment: this smoke guide.
- Daily/weekly: concurrency burst and short load run from `MANUAL_TESTING_GUIDE.md`.
- Before major release or infra changes: full manual guide, including CORS matrix and extended negative flows.
