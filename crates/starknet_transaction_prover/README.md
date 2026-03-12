# Starknet Transaction Prover

A standalone JSON-RPC service that generates STARK proofs for individual Starknet transactions.
The service re-executes a transaction against a finalized block, runs the Starknet virtual OS, and produces a proof with proof facts suitable for on-chain verification.

## Quickstart

```bash
docker run --rm -p 3000:3000 -e RPC_URL=https://your-node.com/rpc/v0_10 <IMAGE>
```

Verify the service is running:

```bash
curl -s -X POST http://localhost:3000 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}'
```

Expected response:

```json
{ "jsonrpc": "2.0", "id": 1, "result": "0.10.0" }
```

## API Reference

The service exposes JSON-RPC 2.0 on the root path (`/`). The full machine-readable spec is at
`resources/proving_api_openrpc.json`.

### `starknet_specVersion`

Returns the API version string.

```bash
curl -s -X POST http://localhost:3000 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}'
```

Response:

```json
{ "jsonrpc": "2.0", "id": 1, "result": "0.10.0" }
```

### `starknet_proveTransaction`

Proves a single Invoke V3 transaction against a finalized block.

**Parameters**

| Name          | Type             | Description                                                                                                                              |
| ------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `block_id`    | `BlockId`        | Block to execute against. Accepts `{"block_number": N}`, `{"block_hash": "0x..."}`, or `"latest"`. Pending blocks are **not** supported. |
| `transaction` | `RpcTransaction` | An Invoke V3 transaction. Declare and DeployAccount are not supported.                                                                   |

**Example request**

```bash
curl -s -X POST http://localhost:3000 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "starknet_proveTransaction",
    "params": {
      "block_id": {"block_number": 700000},
      "transaction": {
        "type": "INVOKE",
        "version": "0x3",
        "sender_address": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        "calldata": ["0x1", "0xabcdef"],
        "signature": ["0xabc", "0xdef"],
        "nonce": "0x5",
        "resource_bounds": {
          "l1_gas":      {"max_amount": "0x0", "max_price_per_unit": "0x0"},
          "l2_gas":      {"max_amount": "0x5f5e100", "max_price_per_unit": "0x0"},
          "l1_data_gas": {"max_amount": "0x0", "max_price_per_unit": "0x0"}
        },
        "tip": "0x0",
        "paymaster_data": [],
        "account_deployment_data": [],
        "nonce_data_availability_mode": "L1",
        "fee_data_availability_mode": "L1"
      }
    }
  }'
```

**Response shape**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "proof": "<base64-encoded proof bytes>",
    "proof_facts": ["0x1a2b3c", "0x4d5e6f"],
    "l2_to_l1_messages": [
      {
        "from_address": "0x1234...",
        "to_address": "0xabcd...",
        "payload": ["0x1", "0x2"]
      }
    ]
  }
}
```

### Transaction requirements

- Only INVOKE V3 transactions (`"type": "INVOKE"`, `"version": "0x3"`) are accepted.
- Fee-related prices and tip must be zero: each resource bound (`l1_gas`, `l2_gas`, `l1_data_gas`)
  must have `max_price_per_unit` set to `"0x0"`, and `tip` must be `"0x0"`. The `max_amount` fields
  may be non-zero. Proving is client-side; no fees are charged. Disable this check with
  `SKIP_FEE_FIELD_VALIDATION=true`.
- The `proof` and `proof_facts` fields are output-only and must be absent from the request (they
  are not part of the `RpcTransaction` input type).

## Errors

| Code     | Name                            | Cause                                                                                    |
| -------- | ------------------------------- | ---------------------------------------------------------------------------------------- |
| `24`     | Block not found                 | Block doesn't exist or a pending block ID was used.                                      |
| `55`     | Account validation failed       | The transaction's `__validate__` entry point reverted. Check the `data` field.           |
| `61`     | Unsupported transaction version | A non-Invoke transaction was sent (Declare, DeployAccount).                              |
| `1000`   | Invalid transaction input       | Invalid request field: non-zero gas prices or tip, or other malformed input. See `data`. |
| `-32005` | Service busy                    | At concurrent proving capacity. Retry later.                                             |
| `-32603` | Internal error                  | Unexpected failure. The `data` field contains diagnostic information.                    |

**Example error response (code 61)**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": 61,
    "message": "The transaction version is not supported",
    "data": "Only Invoke V3 transactions are supported for proving"
  }
}
```

## Configuration

Configuration is accepted via environment variables, CLI flags, or a JSON config file. CLI flags
and environment variables override values from the config file. See
`resources/example-config.json` for a full JSON config reference.

### Environment variables

| Variable                    | CLI Flag                      | Default            | Description                                                                                  |
| --------------------------- | ----------------------------- | ------------------ | -------------------------------------------------------------------------------------------- |
| `RPC_URL`                   | `--rpc-url`                   | _(required)_       | Starknet RPC node URL (v0.10 compatible).                                                    |
| `CHAIN_ID`                  | `--chain-id`                  | `SN_MAIN`          | Network: `SN_MAIN`, `SN_SEPOLIA`, or a custom chain ID string.                               |
| `PROVER_PORT`               | `--port`                      | `3000`             | JSON-RPC server port.                                                                        |
| `PROVER_IP`                 | `--ip`                        | `0.0.0.0`          | Bind IP address.                                                                             |
| `MAX_CONCURRENT_REQUESTS`   | `--max-concurrent-requests`   | `2`                | Max parallel proving requests. Excess requests receive error `-32005`.                       |
| `MAX_CONNECTIONS`           | `--max-connections`           | `10`               | Max simultaneous TCP connections.                                                            |
| `SKIP_FEE_FIELD_VALIDATION` | `--skip-fee-field-validation` | `false`            | Allow non-zero gas prices and tip in requests.                                               |
| `STRK_FEE_TOKEN_ADDRESS`    | `--strk-fee-token-address`    | _(auto per chain)_ | Override STRK fee token address (hex). Useful for custom environments that share a chain ID. |
| `PREFETCH_STATE`            | `--prefetch-state`            | `false`            | Simulate transactions before proving to prefetch state and reduce RPC calls.                 |
| `CONFIG_FILE`               | `--config-file`               | —                  | Path to JSON config file. See `resources/example-config.json`.                               |

### TLS / HTTPS

| Variable        | CLI Flag          | Description                  |
| --------------- | ----------------- | ---------------------------- |
| `TLS_CERT_FILE` | `--tls-cert-file` | TLS certificate chain (PEM). |
| `TLS_KEY_FILE`  | `--tls-key-file`  | TLS private key (PEM).       |

Both variables must be set together. When both are present the server uses HTTPS; when both are
absent it uses plain HTTP. Setting only one is an error.

### CORS

| Variable            | CLI Flag              | Description                                                   |
| ------------------- | --------------------- | ------------------------------------------------------------- |
| `CORS_ALLOW_ORIGIN` | `--cors-allow-origin` | Comma-separated list of allowed origins, or `*` to allow all. |
| —                   | `--no-cors`           | Disable CORS (overrides any origins set in the config file).  |

CORS is disabled by default. `--no-cors` and `--cors-allow-origin` are mutually exclusive.

### Docker example with common options

```bash
docker run --rm -p 3000:3000 \
  -e RPC_URL=https://your-node.com/rpc/v0_10 \
  -e CHAIN_ID=SN_MAIN \
  -e MAX_CONCURRENT_REQUESTS=4 \
  -e CORS_ALLOW_ORIGIN=https://app.example.com \
  <IMAGE>
```

### Logging

The service uses the `RUST_LOG` environment variable (via `tracing-subscriber`).

```bash
# Default — service logs at debug, noisy proving libraries at warn:
docker run ... <IMAGE>

# Verbose — all crates at debug:
docker run -e RUST_LOG=debug ... <IMAGE>

# Quiet — warnings and errors only:
docker run -e RUST_LOG=warn ... <IMAGE>
```

## Limitations

- Invoke V3 only — Declare and DeployAccount transactions are not supported.
- Finalized blocks only — pending blocks are not supported as the `block_id`.
- One transaction per request — batch proving is not available.
- Nightly Rust required for the Stwo prover — this is handled automatically in the Docker image.

## Building the Docker image

```bash
# From the repository root:
docker build -f crates/starknet_transaction_prover/Dockerfile -t tx_prover:latest .
docker run --rm -p 3000:3000 -e RPC_URL=https://your-node.com/rpc/v0_10 tx_prover:latest
```

The Dockerfile uses a multi-stage build with `cargo-chef` for dependency caching. The nightly
Rust toolchain is installed automatically from `rust-toolchain.toml`. The final image contains
only the runtime binary and required resources.

### CPU-specific builds

Building with `-C target-cpu` set to the host microarchitecture provides a meaningful proving
performance boost. Pass `RUSTFLAGS` as a build arg:

```bash
# Example: optimized for AMD EPYC Turin (GKE c4d nodes)
docker build -f crates/starknet_transaction_prover/Dockerfile \
  --build-arg RUSTFLAGS="-C target-cpu=znver5" \
  -t tx_prover:latest .
```

A convenience script is available for parameterized builds:

```bash
./scripts/build_starknet_transaction_prover.sh --rustflags "-C target-cpu=znver5"
```
