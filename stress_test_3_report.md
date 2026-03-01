# Stress Test Report — Test 3: Invoke Transactions – Large Execution + StateDiff Touch

## Test Overview

| Field | Details |
|---|---|
| **Isolated Component Goal** | Concurrency / Blockifier |
| **Motivation** | Create stress on block-building |
| **Task** | Have transactions touch the same storage slot to burden concurrency in Blockifier |
| **Notes** | Isolate away from committer and mempool limitations by ensuring StateDiffs and Calldata are at a minimum |
| **Test Duration** | 30 minutes (13:30 – 14:00 IST, March 5 2026) |

---

## Key Results

| Metric | Value |
|---|---|
| **Highest successful Batched TPS** | 444 |
| **Average Batched TPS** | 399 |
| **Gateway TPS (peak)** | 480 |
| **Gateway TPS (average)** | 443 |
| **Mempool TPS (peak)** | 475 |
| **Mempool TPS (average)** | 434 |
| **HTTP Server Add Tx Latency** | 17.5 ms (p50) – 24.7 ms (p95) |
| **Cende blob size (average)** | 3.6 MB |
| **Number of txs per block (average)** | 487 |
| **Write Blob latency (Cende)** | 0.84 s (p50) – 2.23 s (p95) |
| **Commit Block latency** | 1.54 s (p50) – 2.41 s (p95) |

---

## Block Production

| Metric | Value |
|---|---|
| **Total blocks produced** | 1,475 |
| **Total transactions processed** | 712,235 |
| **Blocks/min (steady state)** | ~49 |

### Block Close Reasons

| Reason | Count | % |
|---|---|---|
| `full_block` | 1,435 | 97.3% |
| `idle_execution_timeout` | 40 | 2.7% |
| `deadline` | 0 | 0% |

---

## Health & Stability

| Metric | Value |
|---|---|
| **Consensus round > 0** | 0 (no timeouts, no reproposals) |
| **Reverted blocks** | 0 |
| **Failed proposals** | 0 |
| **Gateway failures** | 16,497 (~3.2% of received) |

Gateway failures were spread throughout the test at ~10–23/s — likely mempool backpressure.

---

## Resource Usage

| Metric | Value |
|---|---|
| **Sequencer-core CPU** | ~2.5 cores (steady state) |
| **Sequencer-core Memory** | ~0.34 GB avg |

---

## Bottleneck Analysis

The **Blockifier execution itself** is the bottleneck. Blocks consistently close at the gas limit (`full_block` 97.3% of the time), producing ~49 blocks/min at steady state. CPU usage is moderate (~2.5 cores), suggesting the bottleneck lies in **concurrency contention on the shared storage slot** rather than raw compute.

The commit block latency (~1.54 s p50) and write blob latency (~0.84 s p50) are well within acceptable bounds and do not constrain throughput. The Cende blob size is small (3.6 MB vs. 50.5 MB in the large calldata test), confirming that calldata/state-diff overhead was minimized as intended.

Gateway failures (3.2%) indicate the load generator slightly exceeded the system's ingestion capacity — the sequencer was block-building at max concurrency throughput and rejecting overflow transactions.

