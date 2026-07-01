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
{ "jsonrpc": "2.0", "id": 1, "result": "0.10.3-rc.2" }
```

## API Reference

The service exposes JSON-RPC 2.0 on the root path (`/`). The full machine-readable spec is the
`proving-api/starknet_proving_api_openrpc.json` document in
[starknet-specs](https://github.com/starkware-libs/starknet-specs), pinned to the revision recorded
in `resources/starknet_specs_rev.txt`. Two HTTP-only side endpoints are also served (`GET /health`
and `GET /metrics`) — see the [Observability](#observability) section.

### `starknet_specVersion`

Returns the API version string.

```bash
curl -s -X POST http://localhost:3000 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_specVersion","params":[]}'
```

Response:

```json
{ "jsonrpc": "2.0", "id": 1, "result": "0.10.3-rc.2" }
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
- **Prices and tip must be zero**: since proving is client-side and no fees are charged, each
  resource bound (`l1_gas`, `l2_gas`, `l1_data_gas`) must have `max_price_per_unit` set to `"0x0"`,
  and `tip` must be `"0x0"`. Disable this check with `SKIP_FEE_FIELD_VALIDATION=true`.
- **`l2_gas.max_amount` must be non-zero**: this value is the gas limit the OS enforces on the
  transaction. Set it to the value returned by `starknet_estimateFee`, or use `"0x5f5e100"`
  (100,000,000) as a safe upper bound — this is sufficient for approximately 1 million Cairo steps.
- **`l1_gas.max_amount` and `l1_data_gas.max_amount`** do not affect OS execution and can be any
  value (including zero).
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
| `MAX_CONCURRENT_REQUESTS` | `--max-concurrent-requests` | `2` | Max proving requests running in parallel (worker slots). Beyond this, requests queue (see `MAX_QUEUED_REQUESTS`); they receive error `-32005` only when the queue is full. Bound by available CPU/memory. Must be >=1. |
| `MAX_QUEUED_REQUESTS` | `--max-queued-requests` | `8` | Requests that may wait FIFO for a worker slot beyond `MAX_CONCURRENT_REQUESTS`. When this buffer is full, further requests are rejected with `-32005`. `0` reproduces immediate rejection once all workers are busy. |
| `QUEUE_WAIT_TIMEOUT_MILLIS` | `--queue-wait-timeout-millis` | `30000` | Backstop: how long a queued request waits for a worker slot before a `-32005` rejection, so a stuck worker can't pin a waiter's connection indefinitely. |
| `MAX_CONNECTIONS` | `--max-connections` | `10` | Max simultaneous TCP connections accepted by the server. Must be >=1. |
| `SKIP_FEE_FIELD_VALIDATION` | `--skip-fee-field-validation` | `false` | When `true`, allows non-zero gas prices and tip in requests. By default the service rejects them because proving is client-side and no fees are charged. |
| `STRK_FEE_TOKEN_ADDRESS` | `--strk-fee-token-address` | _(auto per chain)_ | Override the STRK fee token contract address (hex). Only needed for custom networks that share a standard chain ID but use a different fee token. |
| `PREFETCH_STATE` | `--prefetch-state` | `false` | Simulate the transaction before proving to prefetch state from the RPC node. Reduces the number of RPC calls during the actual proof run at the cost of one extra simulation. |
| `USE_LATEST_VERSIONED_CONSTANTS` | `--use-latest-versioned-constants` | `true` | Use the latest versioned constants rather than block-version constants. Must match the OS version used by the prover. |
| `COMPILED_CLASS_CACHE_SIZE` | `--compiled-class-cache-size` | `600` | Number of compiled Sierra contract classes to keep in an in-memory LRU cache. Higher values reduce RPC fetches for repeated contracts. |
| `MAX_REQUEST_BODY_SIZE` | `--max-request-body-size` | `5242880` (5 MiB) | Maximum size of an incoming JSON-RPC request body in bytes. Requests exceeding this limit are rejected before parsing. |
| `CONFIG_FILE` | `--config-file` | — | Path to a JSON config file. Fields use snake_case names matching `resources/example-config.json`. Values in the file are overridden by env vars and CLI flags. |
| `RUST_LOG` | — | _(see Logging)_ | Controls log verbosity via `tracing-subscriber`. |
| `LOG_FORMAT` | `--log-format` | `text` | Log output format. Use `json` in production so aggregators (e.g. Datadog) parse fields directly. Accepts `text` or `json`. |
| `HEALTH_MAX_SATURATED_MS` | `--health-max-saturated-ms` | `10000` | How long the service must be continuously rejecting proving requests before `GET /health` flips to 503. See [Observability → /health](#health). |

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
| `max_queued_requests` | `MAX_QUEUED_REQUESTS` | integer |
| `queue_wait_timeout_millis` | `QUEUE_WAIT_TIMEOUT_MILLIS` | integer |
| `max_connections` | `MAX_CONNECTIONS` | integer |
| `validate_zero_fee_fields` | inverse of `SKIP_FEE_FIELD_VALIDATION` | bool |
| `strk_fee_token_address` | `STRK_FEE_TOKEN_ADDRESS` | hex string or null |
| `prefetch_state` | `PREFETCH_STATE` | bool |
| `use_latest_versioned_constants` | `USE_LATEST_VERSIONED_CONSTANTS` | bool |
| `compiled_class_cache_size` | `COMPILED_CLASS_CACHE_SIZE` | integer |
| `max_request_body_size` | `MAX_REQUEST_BODY_SIZE` | integer (bytes) |
| `cors_allow_origin` | `CORS_ALLOW_ORIGIN` | array of strings |
| `tls_cert_file` | `TLS_CERT_FILE` | file path or null |
| `tls_key_file` | `TLS_KEY_FILE` | file path or null |
| `health_max_saturated_ms` | `HEALTH_MAX_SATURATED_MS` | integer (ms) |

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

The service uses the `RUST_LOG` environment variable (via `tracing-subscriber`) to control verbosity,
and `LOG_FORMAT` to switch between human-readable text and machine-readable JSON.

```bash
# Default — service logs at debug, noisy proving libraries at warn, text format:
docker run ... <IMAGE>

# Verbose — all crates at debug:
docker run -e RUST_LOG=debug ... <IMAGE>

# Quiet — warnings and errors only:
docker run -e RUST_LOG=warn ... <IMAGE>

# Production — JSON output so log aggregators parse fields directly:
docker run -e LOG_FORMAT=json ... <IMAGE>
```

`LOG_FORMAT=json` emits one JSON object per line with `timestamp`, `level`, `target`, `fields`,
and `span` keys. URLs that may contain credentials in the userinfo component (`rpc_node_url`,
`blocking_check_url`) are redacted to `scheme://host[:port]` everywhere they appear in logs,
including the startup banner and CLI-override messages.

## Compression

**HTTP response compression** — The server automatically compresses responses when the client
sends the `Accept-Encoding` header. Supported codecs: gzip, brotli, zstd. No server-side
configuration is needed. If the header is omitted, responses are sent uncompressed.

```bash
# Auto-negotiate compression (typically gzip/brotli):
curl --compressed -s -X POST http://localhost:3000 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_proveTransaction","params":{...}}'

# Explicitly request zstd:
curl -H 'Accept-Encoding: zstd' -s -X POST http://localhost:3000 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"starknet_proveTransaction","params":{...}}' | zstd -d
```

## Observability

The service exposes three observability surfaces alongside the JSON-RPC API: HTTP probe endpoints
(`/health`, `/metrics`), structured per-request logs with request-id propagation, and graceful
shutdown semantics. All are unauthenticated by design — they're intended for load balancers,
scrapers, and orchestrators, and contain no service state, no transaction data, and no URLs with
credentials.

### `/health`

`GET /health` returns the current health of the service.

| Status | Body | Meaning |
|---|---|---|
| `200 OK` | `{"status":"ok"}` | Service is accepting requests. |
| `503 Service Unavailable` | `{"status":"unhealthy","reason":"saturated"}` | Service has been continuously rejecting proving requests for at least `HEALTH_MAX_SATURATED_MS` (default 10 seconds). Load balancer should drain this pod. |

The 503 path is driven by the same in-process semaphore that returns JSON-RPC error `-32005`
(Service busy). A single rejection does not trigger 503 — the saturation window must hold for the
full `HEALTH_MAX_SATURATED_MS`. A successful request clears the window immediately, so 503 → 200
transitions are fast once load drops.

The response body is intentionally opaque (no timestamps, counters, or upstream URLs) because
`/health` is unauthenticated. The endpoint is served by the outermost tower middleware so probes
bypass CORS, compression, and JSON-RPC parsing.

### `/metrics`

`GET /metrics` returns the Prometheus text-format scrape with all metrics emitted by the service.
Like `/health`, scrapes bypass CORS and JSON-RPC parsing, and the endpoint is unauthenticated.

| Metric | Type | Labels | Description |
|---|---|---|---|
| `prover_build_info` | gauge | `version`, `git_sha` | Always 1. Used to identify the running build from a scrape. |
| `prover_http_requests_total` | counter | `method`, `status` | HTTP request count. `method` is a bounded enum (`GET`/`POST`/`PUT`/`DELETE`/`HEAD`/`OPTIONS`/`PATCH`/`other`); `status` is the HTTP status class (`1xx`–`5xx`, plus `other`/`error`). Excludes `/health` and `/metrics` probes. |
| `prover_http_request_duration_seconds` | histogram | `method` | End-to-end HTTP request latency, sliced by `method` only (no `status`). Excludes `/health` and `/metrics` probes. |
| `prover_http_inflight_requests` | gauge | — | Current count of HTTP requests being handled. Decremented via RAII so panics and cancellations don't leak. |
| `prover_prove_transaction_outcome_total` | counter | `outcome` | Every proving request, whether served or shed, so its total is the shared denominator for all proving rates. `outcome` is a bounded enum: `success`, `failure_validation`, `failure_blocked`, `failure_runner`, `failure_output_parse`, `failure_proving`, `rejected_queue_full`, `rejected_wait_timeout`. The two `rejected_*` values are the busy-rejects (JSON-RPC error `-32005`). |
| `prover_prove_transaction_duration_seconds` | histogram | — | End-to-end proving duration for admitted requests (virtual OS run + STWO proving). Rejected requests never reach it. |
| `prover_os_run_duration_seconds` | histogram | — | Virtual OS execution time, recorded for successful runs only. |
| `prover_stwo_prove_duration_seconds` | histogram | — | STWO proving time, recorded for successful runs only (requires `stwo_proving` feature). |
| `prover_queue_waiting_requests` | gauge | — | Requests admitted to the queue but still waiting for a worker slot. Decremented via RAII on slot acquisition, timeout, or client disconnect. |
| `prover_queue_wait_duration_seconds` | histogram | — | Time a request waited in the queue before acquiring a worker slot (successful acquisitions only). |
| `prover_panics_total` | counter | — | Process panics caught by the global panic hook. Useful for alerting on panic rate without log search. |

Label cardinality is bounded — no user-controlled values become labels.

### Request IDs

Every HTTP request is logged with a `request_id` field. The id is taken from the inbound
`x-request-id` header when it's a short, printable-ASCII token (max 128 bytes); otherwise the
service generates a UUID v4. The (possibly generated) id is echoed back on the response in the
same header so a caller that triggered a failure can quote a single id when reporting it.

Hostile inputs (whitespace, non-printable bytes, oversized values) are dropped and replaced with
a freshly generated id rather than being trusted — this prevents header smuggling and log-field
explosion.

**OHTTP traffic uses two distinct ids, by design.** For an Oblivious HTTP request the id above is
assigned to the *outer envelope*: it is echoed on the (relay-visible) `message/ohttp-res` response
and appears in the outermost access-log line. The *decapsulated inner request* is given a separate,
freshly generated id that is bound to its content-level logs and **never echoed back**. Any
client-supplied id inside the envelope is discarded. This is a security property, not a logging
inconsistency: keeping the relay-visible envelope id distinct from the gateway's content-log id
means a party holding both the relay's records (id → client) and the gateway's logs (id → request
contents) has no shared key to join them, preserving OHTTP unlinkability. Do not "fix" the two ids
to match — that would reintroduce the join key.

### Per-request log line

Each HTTP request produces a single structured log line at `info` level with
`event="http_request"`:

```json
{
  "timestamp": "...",
  "level": "INFO",
  "fields": {
    "event": "http_request",
    "request_id": "a1b2c3d4...",
    "method": "POST",
    "path": "/",
    "status": 200,
    "latency_ms": 1247,
    "message": "HTTP request handled."
  }
}
```

Request bodies are never inspected — transaction calldata is private user data and never reaches
the log stream.

### Startup banner

At startup the service emits a single `info` log with version, git SHA, chain id, redacted RPC
host, and feature flags. Credentials embedded in URLs (e.g. `https://user:secret@rpc.example.com/`)
are stripped down to `scheme://host[:port]` before logging. No fee token address, TLS path, or
transaction-scoped data appears in the banner.

### Shutdown and panics

`SIGTERM` and `SIGINT` trigger a graceful shutdown — the service stops accepting new requests and
waits for in-flight proofs to finish. Both signals produce structured log events
(`shutdown_started`, `shutdown_complete`) so deployment events are visible in the log stream.

A **second** termination signal during graceful shutdown forces `exit(1)` so an operator can
always reclaim a stuck process. This works around a known tokio behavior where dropping the
`Signal` handle silently swallows subsequent signals on the same channel.

Process panics are captured by a global panic hook that emits a structured `event="panic"` log
with location, message, and a forced backtrace, then increments the `prover_panics_total` metric.
The hook does not call `process::abort()` — the existing runtime abort-on-panic behavior is
preserved.

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
