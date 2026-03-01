# Stress Test Report: Invoke Transactions – Large Execution + StateDiff touch (Run 2)

## Test Overview
- **Scenario:** Invoke Transactions – Large Execution + StateDiff touch
- **Isolated Component Goal:** Concurrency/Blockifier
- **Motivation:** Create stress on block-building.
- **Task:** Have transactions touch the same storage slot to burden concurrency in Blockifier.
- **Notes:** Isolate away from committer and mempool limitations by ensuring StateDiffs and Calldata are at a minimum.
- **Date:** 2026-03-05, 20:51–21:18 IST (27 minutes)
- **Namespace:** apollo-stress-tests-10

## Key Results
- **Highest successful TPS:** 500
- **HTTP Server Add Tx Latency:** 18.8 ms (50%) – 43.0 ms (95%)
- **Cende blob size (average):** 3.5 MB
- **Number of txs per block (average):** 500
- **Write Blob latency (cende):** 0.84 s (50%) – 2.24 s (95%)
- **Commit Block latency:** 1.71 s (50%) – 2.42 s (95%)

## Block Production
- **Block close reasons:** 1,279 full_block (100%), 0 idle_execution_timeout (0%), 0 deadline (0%)
- **Total blocks produced:** 1,279
- **Total transactions processed:** 639,500
- **L2 gas per block:** 1,871,887,500 (constant)
- **Sierra gas per block (avg):** ~2,143,515,000

## Health & Stability
- **Gateway failures:** 0 (0%)
- **HTTP server failures:** 0
- **Batcher rejected transactions:** 0
- **Consensus round > 0:** 0 (no timeouts, no reproposals)
- **Reverted blocks / Failed proposals:** 0 / 0

