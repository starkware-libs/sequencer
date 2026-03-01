# Decentralized Logic Capabilities

This document describes the current decentralized sequencer topology, capacity vectors, and observability signals.

## 1) Deployment Topology (What is running)

### Layout and environment

- Layout: `hybrid` (recommended to state exact layout and overlay in use).
- Environment/overlay: `testing/node-0` in this repo sample.
- Deployment pattern:
  - `hybrid`: mix of separated services.
  - `consolidated`: all services in one node/process group.
  - `distributed`: services spread across namespaces.

### Services and replicas (hybrid layout baseline)

From `deployments/sequencer/configs/layouts/hybrid/services/*.yaml`:

- `core`: replicas `1`
- `gateway`: replicas `1`
- `mempool`: replicas `1`
- `committer`: replicas `1`
- `l1`: replicas `1`
- `sierracompiler`: replicas `1`

Stateful in baseline layout:

- `core` has `statefulSet.enabled: true`
- `committer` has `statefulSet.enabled: true`

### Internal component placement (important for decentralization and bottlenecks)

Use `crates/apollo_deployments/resources/services/hybrid/*.json` to document which component runs where (`Enabled`, `Remote`, `LocalExecutionWithRemoteEnabled`, `Disabled`).

At minimum, include:

- Consensus manager
- Batcher
- Gateway + HTTP server
- Mempool
- Committer
- L1 provider + scrapers
- State sync
- Proof manager
- Sierra compiler
- Monitoring endpoint

## 2) Capacity and Performance (What the system can afford)

## 2.1 Core capacity statements to maintain

- Peak accepted TPS (1 minute and 5 minute windows)
- Sustained TPS (15 minute and 60 minute windows)
- P50/P95 add-tx latency
- Avg/P95 txs per block
- Block time (or blocks per minute)
- Max safe load before reject/failure rate rises

## 2.2 Throughput and latency vectors to track

Use these signals (already present in dashboard config):

- Ingestion rate:
  - `http_server_added_transactions_success`
  - `http_server_added_transactions_total`
  - `gateway_transactions_received`
- Batching rate:
  - `batcher_batched_transactions`
- Mempool flow:
  - `mempool_transactions_received`
  - `mempool_txs_committed`
  - `mempool_transactions_dropped`
  - `mempool_pool_size`
  - `mempool_total_size_bytes`
- Latency:
  - `http_server_add_tx_latency_bucket`
  - `gateway_add_tx_latency_bucket`
  - `mempool_transaction_time_spent_until_batched_bucket`
  - `mempool_transaction_time_spent_until_committed_bucket`
  - `batcher_commitment_manager_commit_block_latency_bucket`
  - `cende_write_prev_height_blob_latency_bucket`

## 2.3 Gas and block saturation vectors

- `batcher_l2_gas_in_last_block`
- `batcher_sierra_gas_in_last_block`
- `batcher_proving_gas_in_last_block`
- Block close reasons:
  - `batcher_block_close_reason{block_close_reason="full_block"}`
  - `batcher_block_close_reason{block_close_reason="deadline"}`
  - `batcher_block_close_reason{block_close_reason="idle_execution_timeout"}`

Interpretation:

- Dominant `full_block` indicates saturation on block-building limits.
- Rising `idle_execution_timeout` suggests under-load.
- Rising rejects/failures with stable CPU can indicate queue/backpressure contention.

## 2.4 Reliability and correctness vectors

- `batcher_rejected_transactions`
- `batcher_reverted_transactions`
- `gateway_add_tx_failure`
- `http_server_added_transactions_failure`
- Consensus health:
  - `consensus_round`
  - `consensus_round_advances`
  - `consensus_timeouts`
  - `consensus_build_proposal_failed`
  - `consensus_proposals_invalid`

## 2.5 Infra and decentralization vectors

- Number of nodes per role and per failure domain (zone/region).
- Replica policy and anti-affinity/topology spread.
- Stateful data durability (PVC class, RPO/RTO assumptions).
- Cross-service dependency retries/timeouts.
- Maximum tolerated node/service failures without liveness loss.

## 3) Machine Sizing Checklist (What to document per service)

For each service (`core`, `gateway`, `mempool`, `committer`, `l1`, `sierracompiler`):

- Pod replicas
- Stateful vs stateless
- CPU request/limit
- Memory request/limit
- Disk size/type (if any PVC)
- Network exposure (ClusterIP/LoadBalancer/Ingress)
- Probes and restart behavior
- Horizontal scaling policy (if HPA enabled)
- Component concurrency configs (e.g. `max_concurrency`)

## 4) How to get tx hash and l2_gas for a single transaction

## 4.1 tx_hash from logs

The batcher emits proposal completion lines containing tx hashes and statuses:

- `Finished generating proposal ... , <tx_hash>: Successful|Reverted|Rejected`

It also emits per-tx execution failure/revert lines:

- `Transaction <tx_hash> is reverted during execution ...`
- `Transaction <tx_hash> failed to execute with error: ...`

Practical log filters:

- `"Finished generating proposal"`
- `"Transaction "` and `"failed to execute"`
- `"is reverted during execution"`

## 4.2 l2_gas for a single tx (recommended path: RPC receipt)

Use Starknet RPC receipt endpoint:

- Method: `starknet_getTransactionReceipt` (`getTransactionReceipt` in server trait)

Read:

- `result.execution_resources.total_gas_consumed.l2_gas`

Notes:

- If `total_gas_consumed` is missing for older block formats, fallback behavior may apply.
- The receipt always contains `transaction_hash`, so this is the most reliable tx-focused lookup.

Example request:

```bash
curl -sS -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  --data '{
    "jsonrpc":"2.0",
    "id":1,
    "method":"starknet_getTransactionReceipt",
    "params":{"transaction_hash":"0xYOUR_TX_HASH"}
  }'
```

Example extraction with `jq`:

```bash
jq -r '.result | {tx_hash: .transaction_hash, l2_gas: .execution_resources.total_gas_consumed.l2_gas, status: .execution_status, revert_error: .revert_error}'
```

## 4.3 Block-level l2 gas (aggregate)

- Metric: `batcher_l2_gas_in_last_block`
- Header fields include:
  - `l2_gas_price`
  - `l2_gas_consumed`
  - `next_l2_gas_price`

This is useful for block saturation tracking, but not a replacement for per-tx receipt lookup.

## 5) Suggested minimum SLO table

Fill and maintain:

| Dimension | Target | Alert threshold | Source |
|---|---:|---:|---|
| Accepted TPS (sustained) | TODO | TODO | HTTP/Gateway/Batcher metrics |
| Add-tx latency p95 | TODO ms | TODO ms | HTTP/Gateway latency histograms |
| Rejected tx ratio | TODO % | TODO % | Batcher/Gateway counters |
| Reverted tx ratio | TODO % | TODO % | Batcher counters |
| Consensus timeout rate | TODO | TODO | Consensus timeout metrics |
| Full block ratio | TODO % | TODO % | Block close reason metric |

## 6) Current observed benchmark snapshots (from repo reports)

Populate from stress reports as reference baselines (not production guarantees):

- `stress_test_3_report.md`
- `stress_test_3b_report.md`

Example values observed there:

- Successful TPS up to ~`500`
- Avg tx/block around `500`
- Stable `full_block` dominance in saturation scenario
- L2 gas per block reported around `1,871,887,500` in one run

## 7) Gaps to close (recommended)

- Add a dedicated structured log line for per-tx gas (`tx_hash`, `l2_gas`, `status`) at block finalization.
- Add a dashboard panel for per-tx gas percentiles (from receipts or derived events).
- Add a runbook section with exact Loki queries and JSON extraction snippets.

