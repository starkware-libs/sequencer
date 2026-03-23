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
- Access to a Starknet RPC node on the same chain as the prover (for nonce lookup).
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

- `DUMMY_ACCOUNT_ADDRESS` -- address of a dummy-validate account on the target
  chain (default: the Sepolia Integration dummy account).
- `STRK_TOKEN_ADDRESS` -- STRK token contract address on the target chain
  (default: Sepolia Integration STRK address).
- `KEEP_ARTIFACTS=true` -- preserve temp files for post-mortem inspection.
  Artifacts are also preserved automatically when any check fails.

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
  `crates/starknet_transaction_prover/src/server/rpc_impl.rs` (currently `"0.10.1"`).

### 3.2 Happy path: `starknet_proveTransaction` with synthetic tx

The script constructs a synthetic INVOKE v3 transaction targeting a
`dummy_validate` account (empty signature, zero fees). This avoids the
signature-invalidation problem that occurs when zeroing fee fields on real
chain transactions.

1. Fetch the dummy account's current nonce from the chain RPC.

```bash
DUMMY_ACCOUNT="0x2763d2701f413cf0ad7bc73690297c4594bbfd4632ee5f017eb287051595672"
STRK_TOKEN="0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"

NONCE=$(curl -sS "$CHAIN_RPC_URL" \
  -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"starknet_getNonce\",\"params\":[\"latest\",\"$DUMMY_ACCOUNT\"]}" \
  | jq -r '.result')
```

2. Build and send the prove request. The calldata calls `balanceOf(sender)` on
   the STRK token -- a read-only operation that cannot fail from insufficient
   balance.

```bash
jq -nc \
  --arg sender "$DUMMY_ACCOUNT" \
  --arg strk "$STRK_TOKEN" \
  --arg nonce "$NONCE" \
  '{
    jsonrpc: "2.0", id: 5,
    method: "starknet_proveTransaction",
    params: {
      block_id: "latest",
      transaction: {
        type: "INVOKE", version: "0x3",
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
  }' > /tmp/prove_request_valid.json

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

### 3.4 Compression check

The server applies `CompressionLayer` (gzip, brotli, zstd) to all HTTP
responses. Verify that compressed responses are returned and decode correctly.

```bash
curl -sS -D- "$PROVER_URL" \
  -H 'content-type: application/json' \
  -H 'accept-encoding: gzip' \
  --compressed \
  -d '{"jsonrpc":"2.0","id":2,"method":"starknet_specVersion","params":[]}' \
  2>&1
```

Pass:

- Response headers include `content-encoding` (e.g. `gzip`).
- The JSON body decodes correctly and `result` matches the expected spec version.

### 3.5 Concurrency/recovery quick check

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
