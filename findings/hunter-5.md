# Bug Hunter 5 Findings

## Files Examined

- `crates/apollo_l1_events/src/l1_scraper.rs` — scraper lifecycle, reorg detection, start-block computation
- `crates/apollo_l1_events/src/l1_events_provider.rs` — provider state machine, commit/validate/propose flows
- `crates/apollo_l1_events/src/transaction_manager.rs` — L1 handler tx lifecycle, proposable index, staging epochs
- `crates/apollo_l1_events/src/transaction_record.rs` — per-tx state machine, time-based state transitions
- `crates/apollo_l1_events/src/catchupper.rs` — L2 catch-up sync task and backlog
- `crates/papyrus_base_layer/src/eth_events.rs` — L1 event parsing, Felt/U256 conversion
- `crates/papyrus_base_layer/src/ethereum_base_layer_contract.rs` — Ethereum contract interface, block header fetching, blob fee calculation
- `crates/papyrus_base_layer/src/cyclic_base_layer_wrapper.rs` — multi-URL cycling wrapper
- `crates/apollo_l1_events_config/src/config.rs` — all config structs

---

## Bug 1

**File**: `crates/apollo_l1_events/src/transaction_manager.rs`
**Location**: `fn clear_old_tx_from_consumed_queue`, ~line 264–274
**Description**: Off-by-one in consumed transaction expiry: a transaction consumed at exactly `unix_now - timelock` (the cutoff boundary) is never deleted. The cutoff should be inclusive, but the `split_off` call makes it exclusive.
**Root Cause**: `BTreeMap::split_off(&k)` returns all entries with key `>= k`, keeping them in `still_timelocked`. So a transaction with a consumed timestamp of exactly `cutoff = unix_now - timelock` survives into `still_timelocked` and is not removed. The correct behavior is: if `consumed_at + timelock <= unix_now`, the tx should be deleted, but the code only deletes when `consumed_at + timelock < unix_now`.

```rust
// Current (buggy):
let cutoff =
    unix_now.saturating_sub(self.config.l1_handler_consumption_timelock_seconds.as_secs());
let still_timelocked = self.consumed_queue.split_off(&BlockTimestamp(cutoff));
// split_off keeps entries >= cutoff, so a tx consumed exactly at `cutoff` is NOT deleted.
// Fix: use BlockTimestamp(cutoff + 1) OR use split_off(&BlockTimestamp(cutoff.saturating_add(1)))
```

The practical impact is a one-second delay in garbage collection for consumed transactions — the cleanup is deferred until `unix_now > consumed_at + timelock` instead of `unix_now >= consumed_at + timelock`. The existing test `consuming_tx_deletes_after_timelock` advances the clock by exactly `timelock` seconds, meaning it exercises the off-by-one boundary: the test passes the clock forward by `timelock` which sets `unix_now == consumed_at + timelock`, i.e., `cutoff == consumed_at`, and the tx should be deleted but is not.

**Failing Test**:
```rust
#[test]
fn test_consumed_tx_deleted_at_exact_timelock_expiry() {
    use std::time::Duration;
    use starknet_api::block::BlockTimestamp;
    use crate::test_utils::{l1_handler, ConsumedTransaction, L1EventsProviderContentBuilder};
    use apollo_l1_events_types::{Event, ProviderState};
    use apollo_time::test_utils::FakeClock;
    use crate::L1EventsProviderConfig;
    use std::sync::Arc;

    // A tx consumed at timestamp 0, with a timelock of 10 seconds.
    let tx = l1_handler(1);
    let consumed_at: u64 = 0;
    let timelock: u64 = 10;

    let config = L1EventsProviderConfig {
        l1_handler_consumption_timelock_seconds: Duration::from_secs(timelock),
        ..Default::default()
    };

    // Clock is set to exactly `consumed_at + timelock`, meaning the timelock has JUST expired.
    // At this moment `unix_now == consumed_at + timelock`, so the tx should be eligible for
    // deletion: `consumed_at <= unix_now - timelock` (0 <= 10 - 10 = 0, true).
    let clock = Arc::new(FakeClock::new(consumed_at + timelock));

    // We need a second tx to consume in order to trigger clear_old_tx_from_consumed_queue.
    let trigger_tx = l1_handler(2);

    let mut provider = L1EventsProviderContentBuilder::new()
        .with_clock(clock)
        .with_consumed_txs([ConsumedTransaction {
            tx: tx.clone(),
            timestamp: BlockTimestamp(consumed_at),
        }])
        .with_txs([trigger_tx.clone()])
        .with_config(config)
        .build_into_l1_provider();

    // Consuming the trigger tx forces clear_old_tx_from_consumed_queue to run.
    // At this moment unix_now == consumed_at + timelock, so `tx` should be deleted.
    provider
        .add_events(vec![Event::TransactionConsumed {
            tx_hash: trigger_tx.tx_hash,
            timestamp: BlockTimestamp(consumed_at + timelock),
        }])
        .unwrap();

    // The original tx should have been deleted since its timelock has expired.
    // BUG: the tx is NOT deleted because split_off uses a strictly-less-than boundary,
    // causing the tx with timestamp == cutoff to survive.
    assert!(
        !provider.tx_manager.records.contains_key(&tx.tx_hash),
        "Transaction should be deleted when unix_now == consumed_at + timelock, but it was not"
    );
}
```

**How to Verify**: `SEED=0 cargo test -p apollo_l1_events test_consumed_tx_deleted_at_exact_timelock_expiry`

---

## Bug 2 (Latent / Structurally Fragile)

**File**: `crates/apollo_l1_events/src/transaction_manager.rs`
**Location**: `fn get_txs`, ~line 73–78
**Description**: `skip_while` is used instead of `filter` to exclude staged transactions from the proposable index. `skip_while` is only correct if staged transactions always form a strict prefix of the iteration order (as the comment claims). The invariant is maintained by the current state machine, but if `validate_tx` is ever called in a context that feeds back into `get_txs` within the same epoch (e.g., from a future optimistic-proposer feature, a test that directly manipulates the manager, or a refactor that relaxes the provider state restrictions), the code will silently include already-staged transactions in proposals, causing double-inclusion of L1 handler transactions in a block.

The code comment for `proposable_index` says: "also at any point the staged transactions are a prefix of the structure under this order." This invariant is not enforced structurally and relies entirely on the caller always staging from the front. Since `validate_tx` stages arbitrary transactions by hash, it can break the prefix invariant — but it currently runs in `Validate` state and `get_txs` in `Propose` state, so they do not interleave within the same epoch.

**Root Cause**: The semantically correct operation is `filter`, not `skip_while`. `filter` skips every staged element regardless of position; `skip_while` only skips a staged prefix. If the prefix invariant ever breaks, `get_txs` will return staged transactions.

**No test provided** because this bug does not manifest under the current provider state machine. It is a latent structural issue.

---

## Additional Observations (Not Bugs, but Noteworthy)

1. **`StagingEpoch::increment` signature misleads**: It takes `&mut self` but does not mutate `self`, returning a fresh `StagingEpoch` instead. The method should take `&self`. Not a bug because `rollback_staging` correctly reassigns the result, but it is a misleading API.

2. **Unused dead constants in `fetch_start_block`**: `SAFTEY_MARGIN_NUMERATOR` and `SAFTEY_MARGIN_DENOMINATOR` are declared and `const_assert!`-ed but never used in the calculation. The actual formula `blocks + blocks / 2` happens to equal `blocks * 3 / 2` for integer arithmetic, but the constants serve no purpose and create an illusion that the formula is derived from them.

3. **Division-by-zero if `l1_block_time_seconds` is zero**: In `fetch_start_block`, `self.config.startup_rewind_time_seconds.as_secs() / self.config.l1_block_time_seconds.as_secs()` would panic if `l1_block_time_seconds` is configured as zero. There is no `Validate` impl for `L1EventsScraperConfig` guarding against this. The default is 12 seconds, so this is not a production risk currently.
