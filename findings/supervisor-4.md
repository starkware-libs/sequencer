# Supervisor-4 Validation Report

Covers Bug Hunters 13 (apollo_infra), 14 (apollo_committer), 15 (apollo_consensus_orchestrator), 16 (starknet_committer).

---

## Summary Table

| Bug ID | Title | Verdict | Severity |
|--------|-------|---------|----------|
| 13-1 | Retry logging off-by-one | confirmed | Low |
| 13-2 | Panic on `attempts_per_log = 0` | confirmed | Medium |
| 13-3 | Integer overflow in exponential backoff | suspected | Low |
| 13-4 | Wrong `ServerError` variant for busy response | confirmed | Low |
| 13-5 | `LocalComponentClient::send` not cancellation-safe | confirmed | High |
| 14-1 | `AVERAGE_COMPUTE_RATE` uses `n_writes` numerator | confirmed | Medium |
| 14-2 | Revert of block 0 skips global-root validation | confirmed | Medium |
| 14-3 | `BLOCKS_COMMITTED` counts reverts | confirmed | Low |
| 15-1 | `prune_fee_proposals_window` keeps one block too many | rejected | — |
| 15-2 | `within_margin` uses proposed value as margin basis | confirmed | Low-Medium |
| 15-3 | `initialize_fee_proposals_window` infinite loop | confirmed | High |
| 15-4 | `valid_proposals` populated before fin-mismatch check | confirmed | High |
| 15-5 | `gas_used == gas_target` assertion semantics | rejected | — |
| 16-1 | `get_nodes_count` inflates by counting contract leaves | confirmed | Medium |
| 16-2 | `StateDiff::is_empty()` false positive with empty inner map | confirmed | Low-Medium |
| 16-3 | `DeletedNodes::is_empty()` inconsistent with phantom entries | suspected | Low |

---

## Bug 13-1: Retry Logging Off-by-One

**Verdict**: confirmed

**Rationale**: The loop at `remote_component_client.rs:373` runs `attempt` from 1 to `max_attempts` inclusive. The condition on line 386 is:

```rust
if attempt % attempts_per_log == attempts_per_log - 1
```

With `attempts_per_log = 5`, this fires when `attempt % 5 == 4`, i.e., at attempts 4, 9, 14. The documented intent ("log every N-th failure") and the config description ("Number of attempts between failure log messages") clearly expect logging at multiples of N: 5, 10, 15. The condition is off by one. The default value of `1` (where `attempt % 1 == 0` is always true) masks the bug in normal operation.

The proposed test is legitimate — it directly exercises the condition arithmetic without touching any internals.

**Fix suggestion**: Change `== attempts_per_log - 1` to `== 0`.

---

## Bug 13-2: Panic on `attempts_per_log = 0`

**Verdict**: confirmed

**Rationale**: `RemoteClientConfig` has no `#[validate]` attribute on `attempts_per_log` (verified by inspection; other fields like `keepalive_timeout_ms` carry `#[validate(custom(function = ...))]`). If a config file sets `attempts_per_log: 0`, execution reaches `attempt % 0` (usize modulo zero), which panics unconditionally in both debug and release. A config with `attempts_per_log: 0` is unusual but structurally valid from a JSON/YAML perspective, and the node process would crash on the first failed RPC attempt.

The proposed test (checking that `config.validate()` returns `Err`) is valid — it tests the config validation path exactly as a user would encounter it.

**Fix suggestion**: Add `#[validate(range(min = 1))]` (or a custom validator) on `attempts_per_log` in `RemoteClientConfig`.

---

## Bug 13-3: Integer Overflow in Exponential Backoff

**Verdict**: suspected

**Rationale**: The code at line 395 is:

```rust
retry_interval_ms = (retry_interval_ms * 2).min(self.config.max_retry_interval_ms);
```

The multiplication is unchecked on `u64`. In debug mode, `u64::MAX / 2 + 1` doubled would indeed panic. However, `initial_retry_delay_ms` is a config value, and in any real deployment this will be set to something sane (milliseconds, e.g., 100ms). There is no documented invariant that prevents a large value from being configured, but this requires adversarial or highly unusual config to trigger.

The proposed test is legitimate — it directly models the arithmetic without touching internals. However, the real-world impact depends on whether config validation could cap this value; none exists currently. The bug is real in principle but the scenario is implausible in practice.

**What would make it confirmable**: A config schema that permits values near `u64::MAX / 2` with no validation, or a fuzz/property test showing the multiplication is reachable with production-range inputs. For now this is a latent defensiveness issue rather than a practical bug.

**Fix suggestion**: Use `retry_interval_ms.saturating_mul(2).min(...)`.

---

## Bug 13-4: Wrong `ServerError` Variant for Busy Response

**Verdict**: confirmed

**Rationale**: At `remote_component_server.rs:352-355`, when the server denies a connection because `max_concurrency` is reached, it constructs:

```rust
ServerError::RequestDeserializationFailure(BUSY_PREVIOUS_REQUESTS_MSG.to_string())
```

This is semantically wrong: `RequestDeserializationFailure` means the request body could not be parsed; here the server hasn't even looked at the request. The HTTP status code (503) is correct, but the variant name misleads any client that matches on `ServerError` variants for retry-vs-fail decisions. The comment in the source confirms the intent is "server busy."

No executable test is provided, but the code inspection is unambiguous. The hunter's written justification accurately explains the impact on client-side error handling.

**Fix suggestion**: Add `ServerError::ServerBusy(String)` variant and use it in the rejection path.

---

## Bug 13-5: `LocalComponentClient::send` Not Cancellation-Safe

**Verdict**: confirmed

**Rationale**: `local_component_client.rs:50-54` sends the request (including the one-shot `res_tx`) into the server's channel, then awaits `res_rx.recv()`. If the outer future is dropped at the cancellation point between those two lines (e.g., via `tokio::time::timeout` or `tokio::select!`), `res_rx` is dropped. The server later calls `tx.send(response).await.expect(...)` at `local_component_server.rs:428`, which panics because `res_rx` is gone.

The comment at line 426-427 in the server explicitly acknowledges: "This might result in a panic if the client has closed the response channel, which is considered a bug." This confirms the bug is known but unmitigated.

The proposed test is legitimate: it uses idiomatic Tokio (`timeout` around a `send` call on a client with a slow component), which is exactly how a real caller would trigger cancellation. The test does not reach into any private internals — it uses the public API. The test correctly predicts the server task will finish (panic) after the timeout fires.

**Fix suggestion**: Either (a) make the server's response send non-panicking (use `let _ = tx.send(...)`) and have the client detect the dropped channel gracefully, or (b) document the `send` function as not cancel-safe and ensure all callers wrap it with an abort handle rather than dropping.

---

## Bug 14-1: `AVERAGE_COMPUTE_RATE` Uses `n_writes` Numerator

**Verdict**: confirmed

**Rationale**: `committer.rs:680-682` directly confirmed:

```rust
let compute_rate = if durations.compute > 0.0 {
    let rate = *n_writes as f64 / durations.compute;
```

The variable is named `compute_rate` and feeds `AVERAGE_COMPUTE_RATE`. Both `compute_rate` and `write_rate` (lines 687-689) use `*n_writes` as the numerator, differing only in the denominator (`durations.compute` vs `durations.write`). The metric name and dashboard description say "compute entries per second" but there is no separate compute-entry counter — the code uses write entries. This is a copy-paste error.

The proposed test, while it does not execute `update_metrics` directly (it's private), correctly demonstrates the structural problem: `BlockMeasurement` has no separate compute-entry counter, so no correct numerator exists for a distinct compute rate. The assertion is trivially true because both sides are `bm.n_writes`.

**Fix suggestion**: Either track a separate compute-entry count in `BlockMeasurement`, or rename `AVERAGE_COMPUTE_RATE` to something accurate (e.g., `AVERAGE_WRITE_ENTRIES_PER_COMPUTE_SECOND`) and update the dashboard description.

---

## Bug 14-2: Revert of Block 0 Skips Global-Root Validation

**Verdict**: confirmed

**Rationale**: `committer.rs:375` has:

```rust
if let Some(prev_committed_block) = last_committed_block.prev() {
    let stored_global_root = self.load_global_root(prev_committed_block).await?;
    if stored_global_root != revert_global_root { ... return Err(...) }
}
```

When `last_committed_block = BlockNumber(0)`, `.prev()` returns `None` and the entire global-root check is skipped. Any `reversed_state_diff` supplied by the caller is silently applied to the trie, leaving the database in a state that does not correspond to the empty/genesis root. The commit path has a symmetric check via `commit_or_load`, but no equivalent guard protects the revert-block-0 path.

The proposed test is legitimate: it creates a real test committer, commits block 0, then reverts it with a deliberately wrong diff and asserts the revert succeeds without error. No private state is manufactured — this exercises the real committer API.

**Fix suggestion**: After computing `revert_global_root`, when `last_committed_block == 0`, compare it against the known empty-state root (the trie root before any block was committed, which the committer can load from its initial empty state).

---

## Bug 14-3: `BLOCKS_COMMITTED` Counts Reverts

**Verdict**: confirmed

**Rationale**: `committer.rs:656` shows `BLOCKS_COMMITTED.increment(1)` inside `update_metrics`. `update_metrics` is called from both `commit_block_inner` (line 241) and `revert_block_inner` (line 423). The metric description in `metrics.rs:22-26` reads "Number of blocks committed, in commit and revert" — the description acknowledges this but the metric name `blocks_committed` does not. Dashboards or alerting rules using `blocks_committed` to infer chain progress or as a denominator for per-block averages will be skewed during reorgs.

No executable test is provided, but the code inspection is unambiguous. The severity is low since the description does mention reverts — but the naming mismatch is a real source of confusion.

**Fix suggestion**: Option 1: rename the metric to `blocks_processed` and align description. Option 2: add `BLOCKS_REVERTED` incremented only in `revert_block_inner`, and use `BLOCKS_COMMITTED` (incremented only in `commit_block_inner`) as the denominator in Grafana panels.

---

## Bug 15-1: `prune_fee_proposals_window` Keeps One Block Too Many

**Verdict**: rejected

**Rationale**: The hunter initially identifies a supposed bug but then self-corrects: "On re-reading I see this is actually correct." The analysis is right. `split_off(&cutoff)` keeps `[cutoff, ∞)` = `[height - window_size, ∞)`, which is exactly the range `compute_fee_actual(height, window_size)` reads: `[height - window_size, height)`. The prune is correct for the current height, and entries past `height` don't exist yet. No bug exists.

---

## Bug 15-2: `within_margin` Uses Proposed Value as Margin Basis

**Verdict**: confirmed

**Rationale**: `validate_proposal.rs:412`:

```rust
let margin = (number1.0 * margin_percent) / 100;
```

`number1` is always the proposed (network-received, untrusted) value; `number2` is the locally computed reference. Using `number1` as the basis makes the allowed margin proportional to the untrusted input, creating an asymmetric validation band.

The hunter's concrete example is correct: a proposer sending `110` with a reference of `100` and `margin_percent = 10` gets `margin = 11`, diff = 10, passes. A proposer sending `90` with the same reference gets `margin = 9`, diff = 10, fails. Both are equidistant from the reference but receive different treatment depending on which direction the proposer chose. The correct basis is `number2` (the validator's local reference), which is symmetric.

The proposed test is legitimate — it uses the public `within_margin` function with concrete values that demonstrate the asymmetry. No private state is touched. The test will pass (both assertions hold), confirming the asymmetric behavior.

Note: `GAS_PRICE_ABS_DIFF_MARGIN = 1`, so with prices of 90, 100, 110 the absolute-diff bypass (≤ 1) does not apply. The asymmetry is real.

**Fix suggestion**: Change `number1.0 * margin_percent` to `number2.0 * margin_percent` (use the local reference as the basis).

---

## Bug 15-3: `initialize_fee_proposals_window` Infinite Loop

**Verdict**: confirmed

**Rationale**: `sequencer_consensus_context.rs:336-352` shows the loop:

```rust
while let Some(block_number) = pending_heights.pop_front() {
    match self.deps.state_sync_client.get_block(block_number).await {
        Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_))) => {
            pending_heights.push_back(block_number);
            tokio::time::sleep(STATE_SYNC_RETRY_INTERVAL).await;
        }
        Err(e) => return Err(e),
        Ok(block) => ...
    }
}
```

If `get_block` permanently returns `BlockNotFound` for any height in the window (e.g., pruned snapshot, corrupted state_sync DB, block that was skipped), the height is re-queued indefinitely. There is no retry counter, no timeout, and no cancellation token. The function will never return; the node will never start consensus.

The proposed test is legitimate: it configures the mock to always return `BlockNotFound`, wraps the call in a timeout, and asserts the function does not complete. This mirrors exactly how the bug surfaces in a real deployment with a pruned node.

**Fix suggestion**: Add a maximum retry count or a total deadline parameter. After exceeding the limit, return an appropriate error instead of spinning forever.

---

## Bug 15-4: `valid_proposals` Populated Before Fin-Mismatch Check

**Verdict**: confirmed

**Rationale**: `validate_proposal.rs:239-245`:

```rust
let mut valid_proposals = args.valid_proposals.lock().unwrap();
valid_proposals.insert_proposal(args.init, content, &args.proposal_id, finished_info);

if built_block != received_fin.proposal_commitment {
    CONSENSUS_PROPOSAL_FIN_MISMATCH.increment(1);
    return Err(ValidateProposalError::ProposalFinMismatch);
}
```

The proposal is inserted with the **batcher's** commitment (computed from `finished_info.proposal_commitment.partial_block_hash` and `init.fee_proposal_fri` via `proposal_commitment_from`). If `built_block != received_fin.proposal_commitment`, the function returns an error, but the map already holds the entry with the batcher's commitment.

A subsequent `decision_reached` or `get_proposal` call for that height/round with the network's commitment would hit the `assert_eq!` in `get_proposal` at line 168-173 and panic. The comment in the code ("Update valid_proposals before sending fin to avoid a race condition") explains the ordering intent, but the guard for fin-mismatch comes too late.

The proposed test is somewhat complex (requires mocking batcher, building a full `ProposalValidateArguments`), but it does not manufacture unreachable state — it exercises the real `validate_proposal` function through its public interface. The scenario (batcher produces one hash, network sends a different commitment) is a real protocol scenario that must be handled correctly.

**Fix suggestion**: Perform the fin-mismatch check before inserting into `valid_proposals`. If the race condition with `repropose` is a real concern, document it and address it differently (e.g., insert a placeholder and replace it atomically, or hold the lock across the check).

---

## Bug 15-5: `gas_used == gas_target` Assertion Semantics

**Verdict**: rejected

**Rationale**: When `gas_used == gas_target`, `gas_delta = 0`, so `price_change = 0`, and `adjusted_price = price - 0 = price`. The assertion `gas_used <= gas_target && adjusted_price_u256 <= price_u256` correctly passes with equality. The behavior is numerically correct: price stays flat. 

The hunter acknowledges this works correctly ("So the price stays flat because `price_change` happens to be zero") and frames it as a "design/correctness concern" about assertion weakness. The assertion is not wrong — it is a valid inequality that happens to include equality. A potential future regression where `price_change = 1` when `gas_used == gas_target` would indeed not be caught by this assertion, but that is a speculative concern about a hypothetical future bug, not a current bug. The test the hunter proposes would pass with the current code (price is exactly unchanged), which means it documents correct behavior, not a bug.

---

## Bug 16-1: `get_nodes_count` Inflates by Counting Contract Leaves

**Verdict**: confirmed

**Rationale**: `types.rs:256-264` confirmed:

```rust
pub fn get_nodes_count(&self) -> usize {
    self.classes_trie_proof.len()
        + self.contracts_trie_proof.nodes.len()
        + self.contracts_trie_proof.leaves.len()   // <-- leaf data, not inner nodes
        + self.contracts_trie_storage_proofs...
}
```

`contracts_trie_proof.nodes` is a `PreimageMap` (inner trie nodes). `contracts_trie_proof.leaves` is a `HashMap<ContractAddress, ContractState>` (leaf state data). The method name `get_nodes_count` and its usage in `commit.rs:165,189` as the `n_nodes` parameter for witness-fetch measurements makes clear that only inner preimage nodes should be counted. Including leaves inflates the count by the number of accessed contracts on every block commit.

The proposed test is legitimate — it constructs a `StarknetForestProofs` directly with public fields, places 1 inner node and 2 leaves, and asserts the count is 1. No private state is reached.

**Fix suggestion**: Remove `+ self.contracts_trie_proof.leaves.len()` from `get_nodes_count()`. If a combined count (nodes + leaves) is ever needed, add a separate `get_entries_count()` method with explicit documentation.

---

## Bug 16-2: `StateDiff::is_empty()` False Positive with Empty Inner Map

**Verdict**: confirmed

**Rationale**: `input.rs:100-112` confirmed:

```rust
fn len(&self) -> usize {
    // ...
    for storage_map in storage_updates.values() {
        result += storage_map.len(); // counts inner slots, not outer keys
    }
    result
}

pub fn is_empty(&self) -> bool {
    self.len() == 0
}
```

A `StateDiff { storage_updates: { addr: {} }, ... }` has `len() == 0` and `is_empty() == true`. But `accessed_addresses()` iterates `storage_updates.keys()` and would include `addr`. This is an internal inconsistency: `is_empty()` promises nothing to do, but `accessed_addresses()` — used by `actual_storage_updates()` to drive trie operations — would still open a storage trie for `addr`.

The proposed test is legitimate — it uses only public constructors and methods on `StateDiff`.

**Fix suggestion**: Implement `is_empty()` as `self.storage_updates.is_empty() && self.address_to_class_hash.is_empty() && ...`, checking structural emptiness rather than delegating to `len()`.

---

## Bug 16-3: `DeletedNodes::is_empty()` Inconsistent with Phantom Entries

**Verdict**: suspected

**Rationale**: `deleted_nodes.rs:32-36` confirmed:

```rust
pub fn is_empty(&self) -> bool {
    self.classes_trie.is_empty()
        && self.contracts_trie.is_empty()
        && self.storage_tries.values().all(|leaves| leaves.is_empty())
}
```

If `storage_tries` contains `{ addr: HashSet::new() }`, `is_empty()` returns `true` despite the outer map being non-empty. This is logically inconsistent with how `HashMap::is_empty()` works.

However, the hunter correctly notes that `find_deleted_nodes()` has a guard:
```rust
if deleted_leaves_indices.is_empty() { continue; }
```
which prevents phantom entries from being created in production. The bug exists at the type level but is not reachable through normal production code paths.

The proposed test manufactures the phantom state by directly constructing the struct with a public field (`storage_tries: HashMap::from([(addr, HashSet::new())])`). While the struct fields appear to be public (which makes the test syntactically valid), this is a state that cannot arise via the production API. The test proves a type-level inconsistency but not a reachable bug.

**What would make it confirmable**: Evidence that some code path can produce `DeletedNodes` with a non-empty `storage_tries` where any inner set is empty, bypassing the guard in `find_deleted_nodes`.

**Fix suggestion if desired**: Change `is_empty()` to `self.storage_tries.is_empty() && ...` for consistency, and add an invariant comment explaining phantom entries should never exist.
