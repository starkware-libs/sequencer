# Sequencer Bug-Hunt Report

**Date**: 2026-06-19  
**Scope**: 16 crates, 16 hunters, 4 supervisors  
**Total reported**: 65 bugs across 16 crates  
**Confirmed**: 47 | **Suspected**: 8 | **Rejected**: 10

---

## Summary Table — Confirmed Bugs

| ID | Crate | Title | Severity |
|----|-------|-------|----------|
| H1-B1 | apollo_mempool | Rejected tx leaves successor stuck in priority queue | High |
| H4-B2 | apollo_consensus | Late duplicate message drops entire active stream + buffered data | High |
| H5-B1 | apollo_state_sync | `is_cairo_1_class_declared_at` / `is_class_declared_at` missing `verify_synced_up_to` | High |
| H7-B1 | apollo_storage | `to_block_number` silently dropped for contract-address event iteration | High |
| H9-B2 | apollo_mempool_p2p | Transactions permanently lost on non-full broadcast send error | High |
| H13-B5 | apollo_infra | `LocalComponentClient::send` cancellation panics the server task | High |
| H14-B2 | apollo_committer | Revert of block 0 skips global root validation entirely | High |
| H15-B3 | apollo_consensus_orchestrator | `initialize_fee_proposals_window` spins forever on missing block | High |
| H15-B4 | apollo_consensus_orchestrator | `valid_proposals` map polluted before fin-mismatch check, causes later panic | High |
| H1-B2 | apollo_mempool | `CommitHistory::new(0)` panics on first `commit_block` | Medium |
| H3-B1 | apollo_gateway | `P2pPropagatorClientError` returns internal error in deprecated path (tx succeeded but user told it failed) | Medium |
| H4-B1 | apollo_consensus | Unchecked `u32 + u32` overflow in `should_cache_msg` round limit | Medium |
| H6-B1 | blockifier | DA gas fee-balance discount applied even when `n_storage_updates == 0` | Medium |
| H6-B4 | blockifier | Alias counter written spuriously when no aliases allocated on first call | Medium |
| H7-B2 | apollo_storage | `scan_at_block` panics / wraps at `BlockNumber(u64::MAX)` | Medium |
| H8-B1 | apollo_l1_gas_price | `fetch_rate` u64 underflow panics when `timestamp < lag_interval` | Medium |
| H8-B2 | apollo_l1_gas_price | `quantized_timestamp - 1` underflow when quantized value is zero | Medium |
| H8-B4 | apollo_l1_gas_price | Stale-price guard u64 overflow silently bypassed in release | Medium |
| H9-B1 | apollo_mempool_p2p | Broadcast-size metric fires even on failed/dropped sends | Medium |
| H10-B1 | apollo_http_server | Failure metric not incremented when `convert_to_rpc_tx` fails | Medium |
| H11-B1 | apollo_class_manager | LRU eviction causes metrics double-counting on already-stored classes | Medium |
| H11-B3 | apollo_class_manager | Storage error silently swallowed in `add_class` (falls through to unnecessary compile) | Medium |
| H12-B1 | starknet_patricia | `is_left_descendant` panics / wrong answer on zero-length `PathToBottom` | Medium |
| H14-B1 | apollo_committer | `AVERAGE_COMPUTE_RATE` uses `n_writes` numerator (copy-paste bug) | Medium |
| H15-B2 | apollo_consensus_orchestrator | `within_margin` uses untrusted proposed value as margin basis (asymmetric band) | Medium |
| H16-B1 | starknet_committer | `get_nodes_count` inflates result by adding contract-state leaves as if they were inner nodes | Medium |
| H16-B2 | starknet_committer | `StateDiff::is_empty` false positive when contract present with no storage slots | Medium |
| H2-B1 | apollo_batcher | `recv_many` drops all buffered txs when invalid L1Handler encountered mid-batch | Low |
| H2-B2 | apollo_batcher | `panic!()` at `batcher.rs:603` should be `Err(BatcherError::InternalError)` | Low |
| H2-B4 | apollo_batcher | `get_proposal_content` collapses all errors to opaque `InternalError` | Low |
| H3-B2 | apollo_gateway | Felt arithmetic in nonce range check wraps near field prime, rejects valid txs | Low |
| H3-B3 | apollo_gateway | `max_nonce_for_validation_skip` config field hardcoded, config has no effect | Low |
| H3-B4 | apollo_gateway | `mempool_client_result_to_gw_spec_result` is dead code, never called | Low |
| H3-B5 | apollo_gateway | Combined calldata + proof_facts length reported as `calldata_length` in error | Low |
| H7-B3 | apollo_storage | `unreachable!` messages in `get_starknet_version` / `revert_header` have inverted comparison | Low |
| H8-B3 | apollo_l1_gas_price | `matches!(ret, ...)` in test missing `assert!` — assertion is dead | Low |
| H8-B5 | apollo_l1_gas_price | `L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK` metric is off by one (next-to-scrape vs last scraped) | Low |
| H9-B3 | apollo_mempool_p2p | `max_transaction_batch_size = 0` silently disables auto-flush | Low |
| H9-B4 | apollo_mempool_p2p | `continue_propagation` is a permanent no-op stub in production | Low |
| H10-B2 | apollo_http_server | `Regex::new(...)` recompiled on every error response (DoS amplification) | Low |
| H10-B4 | apollo_http_server | Malformed JSON to RPC endpoint not counted in `ADDED_TRANSACTIONS_TOTAL` | Low |
| H11-B2 | apollo_class_manager | `validate_class_version` runs after expensive compile instead of before | Low |
| H12-B2 | starknet_patricia | `SortedLeafIndices` sorts but doesn't dedup; `bisect_left`/`bisect_right` assume uniqueness | Low |
| H13-B1 | apollo_infra | Retry logging off-by-one (logs at N-1, 2N-1 instead of N, 2N) | Low |
| H13-B2 | apollo_infra | `attempts_per_log = 0` config causes `% 0` panic | Low |
| H13-B4 | apollo_infra | `ServerError::RequestDeserializationFailure` used for "server busy" (HTTP 503) | Low |
| H14-B3 | apollo_committer | `BLOCKS_COMMITTED` metric incremented on reverts, misleading dashboards during reorgs | Low |

---

## Suspected Bugs (8)

| ID | Crate | Title | Why Suspected |
|----|-------|-------|---------------|
| H1-B3 | apollo_mempool | `remove_by_address` can evict wrong queued nonce | Structural concern valid; no concrete reachable test |
| H4-B4 | apollo_consensus | Observer SHC panics on `VoteBroadcasted` | Only reachable via incorrect routing not possible through normal dispatch |
| H5-B3 | apollo_state_sync | `get_latest_block_header` can return `None` while block number is `Some` | Requires internal storage manipulation to reproduce |
| H6-B2 | blockifier | Reverted nested inner calls don't clear events correctly | Real incomplete DFS filter; no executable test provided |
| H10-B3 | apollo_http_server | `validate_supported_tx_version_str` can match wrong `"version":` | Affects error messaging only; no runnable test |
| H11-B4 | apollo_class_manager | Crash between file-write and DB-marker leaves class permanently unwritable | Real TOCTOU; requires process-kill simulation |
| H13-B3 | apollo_infra | Exponential backoff overflow with near-`u64::MAX` initial delay | Requires adversarial config values |
| H16-B3 | starknet_committer | `DeletedNodes::is_empty` false for phantom entry | Guarded in production; test constructs unreachable state |

---

## Bug Details

### H1-B1 — apollo_mempool: Rejected tx leaves successor permanently stuck
**File**: `crates/apollo_mempool/src/mempool.rs`, lines 548–676  
**Severity**: High  
**Root cause**: During `commit_block`, when a tx at nonce N commits and the tx at nonce N+1 is rejected, `remove_rejected_txs` removes N+1 from the pool and queue via `remove_by_address`. But the tx at N+2 — now the next eligible — is never re-inserted into the priority queue. The account is marked as having a gap, so it's excluded from future scheduling indefinitely. Only a new `add_tx` call for that address would unblock it.  
**Fix**: After removing a rejected nonce, check if a successor nonce exists in the pool and re-enqueue it.  
**Test**: See `findings/hunter-1.md`.

---

### H4-B2 — apollo_consensus: Late duplicate drops entire active stream
**File**: `crates/apollo_consensus/src/stream_handler.rs`, lines 518–523  
**Severity**: High  
**Root cause**: When a duplicate message arrives (`message_id < next_message_id`), the `Ordering::Less` arm returns `None` — the stream-finished sentinel. This drops the entire `StreamData`, losing all buffered future messages. The next legitimate message then re-creates the stream from scratch and sends a duplicate `Receiver` to the application.  
**Fix**: Return `Some(data)` in the `Less` arm to preserve state; ignore the duplicate silently.  
**Test**: See `findings/hunter-4.md`.

---

### H5-B1 — apollo_state_sync: `is_class_declared_at` missing sync guard
**File**: `crates/apollo_state_sync/src/lib.rs`, lines 299–337  
**Severity**: High  
**Root cause**: Every other state-reading method calls `verify_synced_up_to(block_number)` first. `is_cairo_1_class_declared_at` and `is_class_declared_at` do not. A node synced to block 50 will return `Ok(false)` for block 100 instead of `Err(BlockNotFound(100))`.  
**Fix**: Add `self.verify_synced_up_to(block_id)?;` at the start of both methods.  
**Test**: See `findings/hunter-5.md`.

---

### H7-B1 — apollo_storage: `to_block_number` silently dropped for contract-address iteration
**File**: `crates/apollo_storage/src/body/events.rs`, lines 124–126  
**Severity**: High  
**Root cause**: When `iter_events` is called with a contract address filter, it dispatches to `EventIterByContractAddress`, which has no `to_block_number` field. The caller's stop-block is silently discarded and events past the requested boundary are returned.  
**Fix**: Add `to_block_number: Option<BlockNumber>` to `EventIterByContractAddress` and enforce it in `next()`.  
**Test**: See `findings/hunter-7.md`.

---

### H9-B2 — apollo_mempool_p2p: Transactions permanently lost on send error
**File**: `crates/apollo_mempool_p2p/src/propagator/mod.rs`, line 118  
**Severity**: High  
**Root cause**: `transaction_queue.drain(..)` empties the queue *before* the fallible `broadcast_message` call. If the channel is disconnected (non-full error), the function returns `Err` but the drained transactions are gone — no re-enqueue, no log.  
**Fix**: Drain into a local buffer, only clear the queue on success.  
**Test**: See `findings/hunter-9.md`.

---

### H13-B5 — apollo_infra: `LocalComponentClient` cancellation panics server
**File**: `crates/apollo_infra/src/component_client/local_component_client.rs:53`, `crates/apollo_infra/src/component_server/local_component_server.rs:428`  
**Severity**: High  
**Root cause**: If `LocalComponentClient::send()` is cancelled (e.g., via `tokio::time::timeout`) after the request is enqueued but before the response arrives, the one-shot `res_rx` is dropped. When the server finishes and calls `tx.send(response).await.expect("Response connection should be open.")`, it panics, crashing the server task permanently. The source code itself labels this "a bug".  
**Fix**: Use `tx.send(response).ok()` on the server side, or use a cancellation token.  
**Test**: See `findings/hunter-13.md`.

---

### H14-B2 — apollo_committer: Revert block 0 skips global root validation
**File**: `crates/apollo_committer/src/committer.rs`, lines 375–389  
**Severity**: High  
**Root cause**: Post-revert validation compares the resulting root against `prev_committed_block.prev()`. For block 0, `prev()` returns `None` and the entire validation is skipped — any `reversed_state_diff` is accepted without checking the known empty-state root.  
**Fix**: On `None`, validate against the hardcoded empty-trie root constant.  
**Test**: See `findings/hunter-14.md`.

---

### H15-B3 — apollo_consensus_orchestrator: Infinite spin on missing block
**File**: `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`, lines 336–353  
**Severity**: High  
**Root cause**: If a block is pruned or corrupted, `initialize_fee_proposals_window` pushes it back onto the `VecDeque` and retries with a 500ms sleep — forever. No retry limit, no total deadline, no cancellation.  
**Fix**: Add a max retry count and return an error after exhausting retries.  
**Test**: See `findings/hunter-15.md`.

---

### H15-B4 — apollo_consensus_orchestrator: `valid_proposals` polluted before fin check
**File**: `crates/apollo_consensus_orchestrator/src/validate_proposal.rs`, lines 239–248  
**Severity**: High  
**Root cause**: `valid_proposals.insert_proposal(...)` is called before the `built_block != received_fin.proposal_commitment` check. On mismatch, the proposal is inserted with the batcher's commitment but consensus will later call `get_proposal` with the network commitment — triggering an `assert_eq!` panic.  
**Fix**: Move the `insert_proposal` call to after the fin-mismatch check.  
**Test**: See `findings/hunter-15.md`.

---

### H8-B1/B2 — apollo_l1_gas_price: Two u64 underflow panics in oracle
**File**: `crates/apollo_l1_gas_price/src/exchange_rate_oracle.rs`, lines 218, 237  
**Severity**: Medium  
**Root cause (B1)**: `timestamp - lag_interval_seconds` uses plain subtraction; panics in debug when timestamp < lag.  
**Root cause (B2)**: `quantized_timestamp - NUMBER_OF_TIMESTAMPS_BACK` on a zero `quantized_timestamp` wraps to `u64::MAX`, causing a guaranteed cache miss.  
**Fix**: Use `checked_sub` / `saturating_sub` with proper error handling.  
**Test**: See `findings/hunter-8.md`.

---

### H12-B1 — starknet_patricia: `is_left_descendant` panic on zero-length path
**File**: `crates/starknet_patricia/src/patricia_merkle_tree/node_data/inner_node.rs`, line 173  
**Severity**: Medium  
**Root cause**: `self.length.0 - 1` on a `u8` with `length == 0` overflows. `PathToBottom::new_zero()` is public API; `get_path_to_descendant` also returns zero-length paths. Crafted preimage data reaching this code could DoS the node.  
**Fix**: Guard `if self.length.0 == 0 { return false; }` before the shift.  
**Test**: See `findings/hunter-12.md`.

---

*Full hunter and supervisor reports are in `findings/hunter-*.md` and `findings/supervisor-*.md`.*
