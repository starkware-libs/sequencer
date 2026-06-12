# Bug Hunter 14 Findings

## Files examined

- `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs` — main context logic, proposal lifecycle, `set_height_and_round`, `BuiltProposals`
- `crates/apollo_consensus_orchestrator/src/build_proposal.rs` — proposal building pipeline
- `crates/apollo_consensus_orchestrator/src/validate_proposal.rs` — proposal validation pipeline
- `crates/apollo_consensus_orchestrator/src/utils.rs` — `truncate_to_executed_txs`, gas price utilities, retrospective block hash
- `crates/apollo_consensus_orchestrator/src/dynamic_gas_price/mod.rs` — fee proposal computation
- `crates/apollo_consensus_orchestrator/src/fee_market/mod.rs` — EIP-1559 base fee calculation
- `crates/apollo_consensus_orchestrator/src/cende/mod.rs` — Aerospike blob pipeline
- `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context_test.rs` — existing tests
- `crates/apollo_consensus_orchestrator/src/utils_test.rs` — existing tests
- `crates/apollo_consensus_orchestrator/src/test_utils.rs` — test helpers

---

## Bug 1 — Inverted comparison arms in `set_height_and_round` queue processing

**File**: `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`
**Location**: `fn set_height_and_round`, lines 1092–1102

**Description**:
When `set_height_and_round` is called with a new (higher) round, it iterates `queued_proposals`
(a `BTreeMap<Round, ...>`) to find a proposal queued for the new round, drop stale past-round
entries, and leave future-round entries untouched.  `first_entry()` always returns the
**smallest key** in the map.  The code performs:

```rust
match self.current_round.cmp(entry.key()) {
    std::cmp::Ordering::Less    => { entry.remove(); }          // wrong
    std::cmp::Ordering::Equal   => { to_process = ...; break; } // correct
    std::cmp::Ordering::Greater => return Ok(()),               // wrong
}
```

The `Less` and `Greater` arms have their actions swapped:

| Comparison | Meaning | Correct action | Actual action |
|---|---|---|---|
| `Less` (`current_round < entry.key()`) | Smallest queued key is a **future** round — nothing more to do | `break` / `return` | **Removes** the future-round entry |
| `Greater` (`current_round > entry.key()`) | Smallest queued key is a **stale past** round — should discard | Continue loop after removing | **Returns early**, leaving stale entry in the map |

**Root Cause**:
The semantics of `cmp` applied to the pair `(self.current_round, entry.key())` were inverted
when the arms were written. The iterator starts from the minimum key, so `Greater` means "we
are ahead of the smallest queued round" (stale, drop it), and `Less` means "the smallest
queued round is still in the future" (nothing to process, stop).

**Consequences**:

1. **Silent loss of a queued proposal (scenario A — `Less` arm).**  
   A proposal that arrived early (e.g. for round 4 while at round 1) is queued.  
   When the node later advances to round 2, `first_entry()` returns round 4,
   `current_round.cmp(4)` = `Less`, and the code *removes* the round-4 entry.  
   The `fin_sender` is dropped, so `fin_receiver.await` returns `Err(Canceled)`.  
   If the round-4 proposal would have been correct, it is now gone and consensus
   cannot validate it.

2. **Stale queue entry accumulates (scenario B — `Greater` arm).**  
   A proposal for round 3 is queued while at round 1.  
   The node skips round 3 and advances to round 4.  
   `current_round.cmp(3)` = `Greater`, so the code returns early, leaving the
   round-3 entry in `queued_proposals`.  
   On the next call to `set_height_and_round` (say round 5), the same problem
   recurs. If several rounds are skipped in rapid succession the map accumulates
   all skipped entries and the round-4+ entry is never processed.

In both cases consensus degrades: validators either drop valid proposals from the
proposer (liveness failure) or carry stale state that complicates future lookups.

**Failing Test**:

```rust
/// Tests that a proposal queued for a future round is NOT dropped when the node
/// advances to an intermediate round.
///
/// Scenario: proposals arrive for rounds 2 and 4 while the context is at round 0.
/// The node advances to round 2 first (processes that proposal), then to round 4
/// (should find and process the round-4 proposal).  With the bug, advancing to
/// round 2 removes the round-4 entry via the inverted `Less` arm, so the
/// round-4 fin_receiver is Canceled instead of completing.
#[tokio::test]
async fn queued_proposal_for_future_round_is_not_dropped_on_intermediate_advance() {
    use futures::channel::mpsc;
    use futures::SinkExt;

    let (mut deps, _network) = create_test_and_network_deps();
    // Round 2 and round 4 will each be validated once.
    deps.setup_deps_for_validate(SetupDepsArgs { number_of_times: 2, ..Default::default() });
    let mut context = deps.build_context();
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();

    let prop_part_txs =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    let prop_part_fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: *TEST_PROPOSAL_COMMITMENT,
        executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
        fin_payload: Some(ProposalFinPayload::default()),
    });

    // Queue a proposal for round 2 (future at this point).
    let (mut sender_r2, receiver_r2) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    sender_r2.send(prop_part_txs.clone()).await.unwrap();
    sender_r2.send(prop_part_fin.clone()).await.unwrap();
    let fin_r2 =
        context.validate_proposal(proposal_init(HEIGHT_0, 2), TIMEOUT, receiver_r2).await;

    // Queue a proposal for round 4 (further future).
    let (mut sender_r4, receiver_r4) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    sender_r4.send(prop_part_txs.clone()).await.unwrap();
    sender_r4.send(prop_part_fin.clone()).await.unwrap();
    let fin_r4 =
        context.validate_proposal(proposal_init(HEIGHT_0, 4), TIMEOUT, receiver_r4).await;

    // Advance to round 2: should process the round-2 proposal.
    context.set_height_and_round(HEIGHT_0, 2).await.unwrap();
    assert_eq!(
        fin_r2.await.unwrap(),
        *TEST_PROPOSAL_COMMITMENT,
        "round-2 proposal should complete successfully"
    );

    // Advance to round 4: should process the round-4 proposal — NOT cancel it.
    // With the bug the round-4 entry was silently removed when we advanced to round 2
    // (Less arm), so fin_r4.await returns Err(Canceled) instead of Ok(commitment).
    context.set_height_and_round(HEIGHT_0, 4).await.unwrap();
    assert_eq!(
        fin_r4.await.unwrap(),
        *TEST_PROPOSAL_COMMITMENT,
        "round-4 proposal should complete successfully after advancing to round 4"
    );
}
```

**How to Verify**: 
```
RUSTC_WRAPPER="" cargo test -p apollo_consensus_orchestrator --features testing queued_proposal_for_future_round_is_not_dropped_on_intermediate_advance
```
The test will fail with `fin_r4.await` returning `Err(Canceled)` because the round-4
entry is removed by the inverted `Less` arm when the node advances to round 2.

---

## Bug 2 — `truncate_to_executed_txs` returns a spurious empty batch when `final_n_executed_txs == 0`

**File**: `crates/apollo_consensus_orchestrator/src/utils.rs`
**Location**: `fn truncate_to_executed_txs`, lines 452–473

**Description**:
When `final_n_executed_txs == 0` (empty block), the function should return an empty
`Vec<Vec<...>>` (no batches).  Instead it returns `vec![vec![]]` — a vector containing
one empty batch.

Trace through the code with one non-empty batch in `content`:
```
remaining_tx_count = 0
for batch in content {          // batch has e.g. 3 txs
    if 0 < 3 {                  // true
        executed_content.push(batch.into_iter().take(0).collect()); // pushes vec![]
        break;
    }
}
// result: vec![vec![]]          expected: vec![]
```

**Root Cause**:
There is no early-exit for the `final_n_executed_txs == 0` case.  The loop always
enters the first batch and pushes an empty vector.

**Impact**:
On reproposal of an empty block, `send_reproposal` iterates the stored transaction
batches and sends one `ProposalPart::Transactions(TransactionBatch { transactions: vec![] })`
over the network before `ProposalPart::Fin`.  A validator receiving this spurious empty
batch will forward it to the batcher via `send_txs_for_proposal` (with an empty txs vec)
before finishing.  This is unexpected — the original proposal had no transaction batches,
but the reproposal produces one.  Depending on batcher behaviour this may cause
`InvalidProposal` or a commitment mismatch between the reproposed and original proposals.

Additionally, when `decision_reached` flattens the stored transactions with
`transactions.into_iter().flatten()`, the empty inner vec produces no elements, so
that path is harmless.  The observable harm is confined to the reproposal stream.

**Failing Test**:

```rust
/// When `final_n_executed_txs == 0`, `truncate_to_executed_txs` must return an empty
/// outer vector, not `vec![vec![]]`.
///
/// With the bug, a non-empty `content` (e.g. one batch of 3 txs) causes the function
/// to return `vec![vec![]]` (one empty batch), which is structurally different from
/// `vec![]` (no batches) and propagates as a spurious empty transaction batch in reproposals.
#[test]
fn truncate_to_executed_txs_zero_returns_empty_outer_vec() {
    use crate::utils::truncate_to_executed_txs;
    use starknet_api::consensus_transaction::InternalConsensusTransaction;

    // Build a non-empty content with one batch of 3 transactions.
    // The exact transactions don't matter; we only care about the batch structure.
    let mut content: Vec<Vec<InternalConsensusTransaction>> =
        vec![INTERNAL_TX_BATCH.clone()];

    let result = truncate_to_executed_txs(&mut content, 0);

    // Expected: no batches at all.
    assert!(
        result.is_empty(),
        "truncate_to_executed_txs with 0 should return an empty Vec, \
         but got {} batch(es): {:?}",
        result.len(),
        result.iter().map(|b| b.len()).collect::<Vec<_>>()
    );
}
```

**How to Verify**:
```
RUSTC_WRAPPER="" cargo test -p apollo_consensus_orchestrator --features testing truncate_to_executed_txs_zero_returns_empty_outer_vec
```
The test will fail with `result.len() == 1` (one empty batch instead of no batches).
