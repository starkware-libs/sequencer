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
{ "jsonrpc": "2.0", "id": 1, "result": "0.10.1" }
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
{ "jsonrpc": "2.0", "id": 1, "result": "0.10.1" }
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
and environment variables override values from the config file.

### Environment variables and CLI flags

| Variable | CLI Flag | Default | Description |
|---|---|---|---|
| `RPC_URL` | `--rpc-url` | _(required)_ | Starknet JSON-RPC node URL (must support v0.10 API). Used to fetch block state during re-execution. |
| `CHAIN_ID` | `--chain-id` | `SN_MAIN` | Target Starknet network. Determines fee token addresses and versioned constants. Accepts `SN_MAIN`, `SN_SEPOLIA`, or a custom chain ID string. |
| `PROVER_PORT` | `--port` | `3000` | TCP port the JSON-RPC server listens on. Must be >=1. |
| `PROVER_IP` | `--ip` | `0.0.0.0` | IP address to bind. Use `127.0.0.1` to restrict to localhost, `0.0.0.0` for all interfaces. |
| `MAX_CONCURRENT_REQUESTS` | `--max-concurrent-requests` | `2` | Max parallel proving requests. Additional requests receive error `-32005`. Bound by available CPU/memory. Must be >=1. |
| `MAX_CONNECTIONS` | `--max-connections` | `10` | Max simultaneous TCP connections accepted by the server. Must be >=1. |
| `SKIP_FEE_FIELD_VALIDATION` | `--skip-fee-field-validation` | `false` | When `true`, allows non-zero gas prices and tip in requests. By default the service rejects them because proving is client-side and no fees are charged. |
| `STRK_FEE_TOKEN_ADDRESS` | `--strk-fee-token-address` | _(auto per chain)_ | Override the STRK fee token contract address (hex). Only needed for custom networks that share a standard chain ID but use a different fee token. |
| `PREFETCH_STATE` | `--prefetch-state` | `false` | Simulate the transaction before proving to prefetch state from the RPC node. Reduces the number of RPC calls during the actual proof run at the cost of one extra simulation. |
| `USE_LATEST_VERSIONED_CONSTANTS` | `--use-latest-versioned-constants` | `true` | Use the latest versioned constants rather than block-version constants. Must match the OS version used by the prover. |
| `COMPILED_CLASS_CACHE_SIZE` | `--compiled-class-cache-size` | `600` | Number of compiled Sierra contract classes to keep in an in-memory LRU cache. Higher values reduce RPC fetches for repeated contracts. |
| `CONFIG_FILE` | `--config-file` | — | Path to a JSON config file. Fields use snake_case names matching `resources/example-config.json`. Values in the file are overridden by env vars and CLI flags. |
| `RUST_LOG` | — | _(see Logging)_ | Controls log verbosity via `tracing-subscriber`. |

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

### JSON config file

The JSON config file uses snake_case field names. All fields are optional and fall back to
built-in defaults. See `resources/example-config.json` for a template.

| JSON field | Corresponds to | Type |
|---|---|---|
| `rpc_node_url` | `RPC_URL` | string |
| `chain_id` | `CHAIN_ID` | string |
| `ip` | `PROVER_IP` | string |
| `port` | `PROVER_PORT` | integer |
| `max_concurrent_requests` | `MAX_CONCURRENT_REQUESTS` | integer |
| `max_connections` | `MAX_CONNECTIONS` | integer |
| `validate_zero_fee_fields` | inverse of `SKIP_FEE_FIELD_VALIDATION` | bool |
| `strk_fee_token_address` | `STRK_FEE_TOKEN_ADDRESS` | hex string or null |
| `prefetch_state` | `PREFETCH_STATE` | bool |
| `use_latest_versioned_constants` | `USE_LATEST_VERSIONED_CONSTANTS` | bool |
| `compiled_class_cache_size` | `COMPILED_CLASS_CACHE_SIZE` | integer |
| `cors_allow_origin` | `CORS_ALLOW_ORIGIN` | array of strings |
| `tls_cert_file` | `TLS_CERT_FILE` | file path or null |
| `tls_key_file` | `TLS_KEY_FILE` | file path or null |

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

## Machine specs

Proving time is highly sensitive to the machine type. We recommend using a **c4d-highcpu-48**
(or equivalent) instance for production workloads.

| Spec | Value |
|------|-------|
| Machine type | c4d-highcpu-48 |
| vCPU | 48 |
| Memory | 96 GB |
| Arch | amd64 (AMD EPYC Turin) |

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
performance boost. Pass `TARGET_CPU` as a build arg:

```bash
# Example: optimized for AMD EPYC Turin (GKE c4d nodes)
docker build -f crates/starknet_transaction_prover/Dockerfile \
  --build-arg TARGET_CPU=znver5 \
  -t tx_prover:latest .
```

A convenience script is available for parameterized builds:

```bash
./scripts/build_starknet_transaction_prover.sh --target-cpu znver5
```

## Kubernetes deployment

Mount a ConfigMap containing your JSON config file and pass its path via `--config-file`.
Override individual values with environment variables in the Deployment spec.

Configuration precedence (highest priority first):

1. CLI flags / container args
2. Environment variables
3. JSON config file (`--config-file`)
4. Built-in defaults
