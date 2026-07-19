# Starknet Transaction Prover Deployment Smoke Testing Guide

This guide is the short checklist to run after each production deployment of
`starknet_transaction_prover`.

Target runtime: 5-10 minutes.

Use `MANUAL_TESTING_GUIDE.md` for deep validation (CORS matrix, load testing,
and wider negative coverage).

## 1. Scope

This smoke plan validates that:

- The service is reachable (`/health`).
- Core JSON-RPC methods respond correctly.
- One real proving flow succeeds end to end.
- Invalid requests (malformed params, a pending block_id) fail with the
  expected JSON-RPC errors.
- The service handles a burst of concurrent requests and recovers cleanly.

Not included in per-deploy smoke:

- Full CORS matrix.
- Sustained load and memory profiling.
- Forcing an actual `-32005` busy rejection, which requires redeploying with
  `max_concurrent_requests: 1` (see `MANUAL_TESTING_GUIDE.md` section 8.1).

Run those periodically (daily/weekly) or before major releases.

## 2. Prerequisites

- A deployed proving service endpoint (for example `http://127.0.0.1:3000`),
  built with the `stwo_proving` cargo feature -- without it,
  `starknet_proveTransaction` is unimplemented and the happy-path check
  (3.8) will always fail.
- The prover config must have `"validate_zero_fee_fields": false` (or pass
  `--skip-fee-field-validation` locally) so that real chain transactions with
  non-zero fees are accepted.
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

The script runs every check in Section 3 and prints a PASS/FAIL summary. It
implements its own RPC/tx-lookup helpers internally -- see the script itself
(`rpc_call_chain`, `rpc_call_prover`, `find_tx_hash`) rather than a copy here.

Optional environment variables:

- `TX_HASH` -- pre-set an `INVOKE` `0x3` tx hash to skip the block scan
  (useful on rate-limited RPCs).
- `LOOKBACK_BLOCKS` -- number of recent blocks to scan (default: 300).
- `KEEP_ARTIFACTS=true` -- preserve temp files for post-mortem inspection.
  Artifacts are also preserved automatically when any check fails.
- `OHTTP_SMOKE=true` -- run the OHTTP key-endpoint check (Section 3.5). The
  script cannot detect the server-side `--ohttp-enabled` flag, so this check
  is opt-in.
- `TLS_MIN_DAYS` -- minimum remaining TLS certificate validity, in days, for
  the certificate check to pass (default: 30).
- `MAX_REQUEST_BODY_SIZE` -- body-size limit in bytes assumed by the
  oversized-body check (default: 5242880, the server default).
- `PROVE_MAX_TIME` -- per-request timeout, in seconds, for
  `starknet_proveTransaction` calls (default: 300). A full SNOS proof can
  take minutes; lightweight calls (specVersion, health, ...) always use a
  fixed 30s timeout regardless of this setting.
- `SPEC_VERSION` -- override the expected spec version. Only needed if the
  script is run away from a repo checkout, where it cannot auto-detect the
  `SPEC_VERSION` constant from `src/server/rpc_impl.rs` (the script exits
  with an error if detection fails and this is not set).

## 3. Per-Deployment Smoke Checks

The script is the source of truth for what each check does; this table is a
quick reference mapping each check to its pass criteria and to the relevant
`MANUAL_TESTING_GUIDE.md` section for manual, step-by-step reproduction.
Checks run in the order listed.

| # | Check | Pass criteria | Script function | Manual deep dive |
|---|-------|----------------|------------------|-------------------|
| 3.1 | Health endpoint | `GET $PROVER_URL/health` returns HTTP 200 with body `{"status":"ok"}`. | `check_health` | §4 (intro) |
| 3.2 | Spec version | `starknet_specVersion` returns the `SPEC_VERSION` constant from `src/server/rpc_impl.rs` (auto-detected by the script). | `check_spec_version` | §4.1 |
| 3.3 | HTTP compression | Response to a gzip-accepting request includes a `content-encoding` header; body still decodes correctly. Note: the server compresses eligible responses (above tower-http's default size threshold), not unconditionally every response. | `check_compression` | §7 |
| 3.4 | TLS certificate validity (HTTPS only) | Certificate `notAfter` is at least `TLS_MIN_DAYS` (default 30) days away. | `check_tls_certificate` | §10.3 |
| 3.5 | OHTTP key endpoint (opt-in, `OHTTP_SMOKE=true`) | `/ohttp-keys` returns HTTP 200, a non-empty body, and a `cache-control` header with a non-zero `max-age`. | `check_ohttp_keys` | §11.1 |
| 3.6 | Request body size limit | An oversized body is rejected (HTTP 413 or a JSON-RPC error), and the service still answers normally afterward. | `check_body_size_limit` | §12.1 |
| 3.7 | Malformed params | A malformed `starknet_proveTransaction` call returns a JSON-RPC error. | `check_malformed_params` | §5.6 |
| 3.8 | Happy path | A real `INVOKE` `0x3` transaction, sent unmodified, proves successfully (`result.proof` and `result.proof_facts` present). | `build_valid_prove_request`, `check_prove_happy_path` | §4.2 |
| 3.9 | Pending block rejected | Proving against `block_id: "pending"` returns error code `24` (`Block not found`). | `check_pending_block_rejected` | §5.1 |
| 3.10 | Concurrency and recovery | 3 concurrent prove requests all complete with no transport or `-32603` errors, and the service still answers `starknet_specVersion` correctly afterward. This does **not** prove the concurrency limit rejects excess load -- see Section 1. | `check_concurrency_and_recovery` | §8.1-8.2 |

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
