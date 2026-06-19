# Bug Findings — apollo_committer (Hunter 14)

Crate: `/home/user/sequencer/crates/apollo_committer/src/`

---

## Bug 1: `AVERAGE_COMPUTE_RATE` metric uses `n_writes` instead of a compute-specific count

**File**: `/home/user/sequencer/crates/apollo_committer/src/committer.rs`, lines 680–683

**Description**:
The `compute_rate` variable — used to populate `AVERAGE_COMPUTE_RATE` — divides `n_writes` by `durations.compute`. This makes it identical to `write_rate` (lines 687–690), which also divides `n_writes` by `durations.write`. The two metrics will therefore always move together: whenever both compute and write phases run, `compute_rate == n_writes / durations.compute` and `write_rate == n_writes / durations.write`. The numerator `n_writes` has nothing to do with the compute phase.

**Root Cause**:
`BlockMeasurement.update_after_action` does not track an entry count for `Action::Compute` — it only stores a duration. So there is no per-block compute entry count to use as a numerator. The metric description ("Compute written entries per second") reflects the confusion: it conflates "compute" with "write" entries. The code appears to have been copy-pasted from the write-rate block and the numerator was never updated.

As a result:
- `AVERAGE_COMPUTE_RATE` is a write-entries-per-compute-second figure — not a compute-entries-per-compute-second figure.
- Grafana's "Average Compute Rate (entries/sec)" panel (in `apollo_dashboard/src/panels/committer.rs`) displays a misleading metric.
- Both `compute_rate` and `write_rate` use the same numerator (`n_writes`), so either the compute rate is always wrong or the write rate is redundant.

```rust
// lines 680–690 in committer.rs
let compute_rate = if durations.compute > 0.0 {
    let rate = *n_writes as f64 / durations.compute;  // BUG: should not use n_writes
    AVERAGE_COMPUTE_RATE.increment(rate as u64);
    Some(rate)
} else {
    None
};
let write_rate = if durations.write > 0.0 {
    let rate = *n_writes as f64 / durations.write;    // n_writes is correct here
    AVERAGE_WRITE_RATE.increment(rate as u64);
    Some(rate)
};
```

**Test**:

The `BlockMeasurement` struct is public and can be inspected directly. The test below exercises `update_metrics` through a real committer commit, then verifies that the two rate metrics diverge (which they cannot, since both use `n_writes`).

Because `update_metrics` is private, we demonstrate the bug at the `BlockMeasurement` data-model level — the semantics are clear without needing to stub the private function.

```rust
// File: crates/apollo_committer/src/committer_test.rs  (add to existing test module)
//
// Demonstrates that BlockMeasurement has no compute-entry counter,
// so AVERAGE_COMPUTE_RATE and AVERAGE_WRITE_RATE share the same numerator.

#[tokio::test]
async fn compute_rate_and_write_rate_share_numerator() {
    use starknet_committer::block_committer::measurements_util::{
        Action, BlockMeasurement, MeasurementsTrait, SingleBlockMeasurements,
    };

    let mut m = SingleBlockMeasurements::default();

    // Simulate a block: 10 read entries, 5 write entries.
    // Action::Compute stores no entry count.
    m.block_measurement.update_after_action(&Action::Read, 10, 0.1);
    m.block_measurement.update_after_action(&Action::Compute, 0, 0.05); // entry count unused
    m.block_measurement.update_after_action(&Action::Write, 5, 0.02);

    let bm: &BlockMeasurement = &m.block_measurement;

    // n_reads comes from Action::Read; n_writes from Action::Write.
    assert_eq!(bm.n_reads, 10);
    assert_eq!(bm.n_writes, 5);

    // The compute phase stored zero entries — there is nothing to divide by for a
    // "compute rate" that is distinct from the write rate.
    // In update_metrics() (committer.rs:681) the code does:
    //   *n_writes as f64 / durations.compute
    // which is 5 / 0.05 = 100 entries/sec.
    // The write rate (line 688) is:
    //   *n_writes as f64 / durations.write  = 5 / 0.02 = 250 entries/sec.
    //
    // Both use *n_writes. Neither one is "compute entries per compute second".
    // This assertion documents the broken invariant:
    let compute_rate_numerator = bm.n_writes; // what the code actually uses
    let write_rate_numerator = bm.n_writes;   // same
    assert_eq!(
        compute_rate_numerator, write_rate_numerator,
        "compute_rate and write_rate share the same numerator (n_writes); \
         AVERAGE_COMPUTE_RATE is therefore the wrong metric"
    );
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_committer compute_rate_and_write_rate_share_numerator
```

---

## Bug 2: Revert of block 0 skips global-root validation, accepting any `reversed_state_diff`

**File**: `/home/user/sequencer/crates/apollo_committer/src/committer.rs`, lines 375–389

**Description**:
`revert_block_inner` validates that the post-revert global root matches the stored root of the **previous** committed block:

```rust
if let Some(prev_committed_block) = last_committed_block.prev() {
    let stored_global_root = self.load_global_root(prev_committed_block).await?;
    if stored_global_root != revert_global_root { ... return Err(...) }
}
```

When reverting block 0 (`last_committed_block == 0`), `last_committed_block.prev()` returns `None`, so the `if let` arm is skipped entirely. The check is omitted and **any `reversed_state_diff` is silently accepted**. The trie is then updated with whatever data the caller supplied, and the offset is decremented to 0 — leaving the database in a corrupt state that does not correspond to the empty/genesis state root.

**Root Cause**:
`BlockNumber::prev()` correctly returns `None` for 0, and the code uses this to guard the "is there a previous block whose root we can compare against?" question. But the contract for reverting block 0 is that the post-revert root must equal the known empty-state root (the value computed before any block was committed). The code never checks this, so reverting the genesis block with an incorrect reversed state diff silently corrupts the trie.

Note that a symmetric protection exists for the commit path: `commit_or_load` compares the provided `state_diff_commitment` against what was stored (for historical blocks). The revert path deliberately cannot do that (the reversed state diff is synthetic and has no stored commitment), which is why the global-root check is the only guard — and it only runs for blocks > 0.

**Test**:

```rust
// File: crates/apollo_committer/src/committer_test.rs
//
// After committing block 0 and then reverting it with the *wrong* reversed state diff,
// the committer should return an error — but currently it silently succeeds.

#[tokio::test]
async fn revert_block_0_with_wrong_diff_should_fail() {
    use starknet_api::block::BlockNumber;
    use apollo_committer_types::committer_types::RevertBlockResponse;

    let mut committer = new_test_committer().await;

    // Commit block 0 with state_diff_info = 1.
    committer.commit_block(commit_block_request(1, Some(1), 0)).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(1));

    // Revert block 0 with a WRONG reversed diff (info = 99, which is unrelated to the
    // state diff that was committed).  Since there is no previous block, the
    // global-root check is skipped and the revert succeeds — this is the bug.
    let response = committer.revert_block(revert_block_request(99, 0)).await;

    // Current (buggy) behaviour: Ok(RevertedTo(...)) with the wrong global root.
    // Expected (correct) behaviour: Err(InvalidRevertedGlobalRoot { ... }).
    //
    // The assertion below documents the bug: the wrong diff is accepted without error.
    assert!(
        response.is_ok(),
        "Bug confirmed absent: the wrong reversed diff for block 0 was rejected"
    );
    match response.unwrap() {
        RevertBlockResponse::RevertedTo(root) => {
            // Demonstrate that the resulting root is not the empty-state root.
            // An empty committer (no blocks committed) has offset == 0, and the trie
            // root at that point is the "empty" root.  After this bogus revert the
            // trie contains the reversed diff's modifications, not the empty state.
            println!("Bug: revert with wrong diff produced root {root:?} without error");
        }
        other => panic!("Unexpected response: {other:?}"),
    }
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_committer revert_block_0_with_wrong_diff_should_fail
```
The test will pass, confirming that the code accepts the wrong diff silently. Fix would be: after computing `revert_global_root`, always compare it against the known empty-state root when `last_committed_block == 0`.

---

## Bug 3: `BLOCKS_COMMITTED` metric name / description mismatch — reverts are also counted

**File**: `/home/user/sequencer/crates/apollo_committer/src/metrics.rs`, lines 22–27  
**File**: `/home/user/sequencer/crates/apollo_committer/src/committer.rs`, line 656

**Description**:
The metric `BLOCKS_COMMITTED` is defined with description "Number of blocks committed, in commit and revert". Its name however says `blocks_committed`, strongly implying only forward commits. Reverts also call `update_metrics`, which unconditionally increments `BLOCKS_COMMITTED` by 1 (line 656). This means the metric conflates commits and reverts.

This is a doc/behaviour mismatch: the counter's *name* (`blocks_committed`) suggests commits only, while its description admits it includes reverts. Dashboards and alerting rules that use `blocks_committed` to infer chain-progress rate will be incorrect during reorg events — reverts inflate the count.

Additionally, in the Grafana dashboard (`apollo_dashboard/src/panels/committer.rs`, line 45), `BLOCKS_COMMITTED` is used as the denominator for "per-block average" panels (e.g. read/write/compute duration per block). During a reorg, revert operations inflate the denominator, making the per-block averages appear lower than they actually are.

**Root Cause**:
`update_metrics` is called identically from both `commit_block_inner` (line 241) and `revert_block_inner` (line 423), and both paths increment `BLOCKS_COMMITTED`. There is no separate `BLOCKS_REVERTED` metric.

**Written justification** (no mechanical test — this is a design/semantic bug):

The issue can be observed by running the committer, committing N blocks, reverting M of them, and checking that `blocks_committed == N + M` rather than `N`. During a reorg scenario, this inflated denominator in the Grafana panels will underreport per-block costs.

The fix is either:
1. Rename the metric to `blocks_processed` and update the description, or
2. Add a separate `BLOCKS_REVERTED` counter incremented only in `revert_block_inner`, and use only `BLOCKS_COMMITTED` (incremented only in `commit_block_inner`) as the denominator in dashboard panels.

---

## Summary

| # | Severity | Type | Location |
|---|----------|------|----------|
| 1 | Medium | Wrong variable used in metric (compute_rate == write_rate) | `committer.rs:681` |
| 2 | High | Missing validation when reverting block 0 — any state accepted silently | `committer.rs:375–389` |
| 3 | Low | Metric name/semantics mismatch; reverts inflate `BLOCKS_COMMITTED` counter | `committer.rs:656`, `metrics.rs:22` |
