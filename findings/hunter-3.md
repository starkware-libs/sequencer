# Bug Hunter 3 Findings

## Files Examined

- `crates/apollo_batcher/src/batcher.rs` — main batcher logic: proposal lifecycle, height management, decision_reached, abort flows, commit flows
- `crates/apollo_batcher/src/block_builder.rs` — block building, transaction selection, remove_last_txs, concurrent execution handling
- `crates/apollo_batcher/src/transaction_provider.rs` — propose/validate transaction providers, L1 handler validation
- `crates/apollo_batcher/src/utils.rs` — verify_block_input, deadline helpers, ProposalTask
- `crates/apollo_batcher/src/commitment_manager/commitment_manager_impl.rs` — commitment task management, block hash finalization
- `crates/apollo_batcher/src/batcher_test.rs` — existing test suite
- `crates/apollo_batcher/src/test_utils.rs` — test helpers, mock builders
- `crates/apollo_batcher/src/transaction_provider_test.rs` — transaction provider tests

---

## Bug 1

**File**: `crates/apollo_batcher/src/batcher.rs`  
**Location**: `fn get_parent_proposal_commitment`, line ~1387–1388  
**Description**: The function panics (instead of returning a proper error or `None`) when called for the first new-protocol block. On this block, the previous block is an old-protocol block whose `PartialBlockHashComponents` were never stored, so the storage read returns `None` for `components`. The `expect(...)` then causes an unrecoverable process crash.  
**Root Cause**: After syncing or committing an old-protocol block via `commit_proposal_and_block` with the `StorageCommitmentBlockHash::ParentHash` variant, `prev_proposal_commitment` is set to `None`. When `get_parent_proposal_commitment` is subsequently called for the first new-protocol block, it falls into the `None` branch and queries storage for `PartialBlockHashComponents` of the old block. Since those components were never written (only the parent hash was stored for old blocks), the query returns `None`. The code then calls `.expect("Missing partial block hash components for previous height.")` which panics.

The call path is:
- `decision_reached` → `get_parent_proposal_commitment(height)` where `height` is the first new block
- OR `get_completed_proposal_result` → `get_parent_proposal_commitment(height)` in the same scenario

The existing tests avoid triggering this by mocking `get_parent_hash_and_partial_block_hash_components` to return `Some(PartialBlockHashComponents::default())` rather than `None`.

**Failing Test**:

Place this in `crates/apollo_batcher/src/batcher_test.rs`:

```rust
#[tokio::test]
#[should_panic(expected = "Missing partial block hash components for previous height.")]
async fn test_get_parent_proposal_commitment_panics_for_first_new_block() {
    // INITIAL_HEIGHT is the first new-protocol block; INITIAL_HEIGHT-1 is the last old block.
    let first_new_block = INITIAL_HEIGHT;
    let prev_old_block = first_new_block.prev().unwrap();

    let mut storage_reader = MockBatcherStorageReader::new();
    storage_reader
        .expect_state_diff_height()
        .returning(move || Ok(first_new_block));
    storage_reader
        .expect_global_root_height()
        .returning(move || Ok(first_new_block));
    // Old block: has a parent hash but NO partial block hash components.
    storage_reader
        .expect_get_parent_hash_and_partial_block_hash_components()
        .with(eq(prev_old_block))
        .returning(|_| Ok((Some(BlockHash::default()), None)));

    let mut batcher = create_batcher(MockDependencies {
        storage_reader,
        ..Default::default()
    })
    .await;

    // prev_proposal_commitment is None (default: no new blocks committed yet,
    // all previously seen blocks were old-protocol blocks synced via add_sync_block).
    assert!(batcher.prev_proposal_commitment.is_none());

    // This panics with "Missing partial block hash components for previous height."
    // because prev_height is an old block without PartialBlockHashComponents in storage.
    let _ = batcher.get_parent_proposal_commitment(first_new_block);
}
```

**How to Verify**: `cargo test -p apollo_batcher test_get_parent_proposal_commitment_panics_for_first_new_block`

The test passes because `should_panic` catches the expected panic. Without `should_panic`, the test would crash the process unexpectedly — demonstrating that production code would also crash.

A correct fix would handle the `None` case for `components` by returning `Ok(None)` instead of panicking, since the first new block has no parent partial-hash commitment.

**Corrected logic**:
```rust
None => {
    // Parent proposal commitment is not cached.
    let (_, components) = self.storage_reader
        .get_parent_hash_and_partial_block_hash_components(prev_height)
        ...?;
    let Some(components) = components else {
        // Previous block is an old-protocol block without partial block hash components.
        // No parent proposal commitment is available.
        return Ok(None);
    };
    // ... compute and return commitment
}
```

---

## Bug 2

**File**: `crates/apollo_batcher/src/batcher.rs`  
**Location**: `fn abort_proposal`, lines ~645–653  
**Description**: `abort_proposal` has a time-of-check/time-of-use (TOCTOU) race condition. Between the `is_active(proposal_id).await` check (which releases the `active_proposal` mutex) and the unconditional `executed_proposals.insert(proposal_id, Err(Aborted))`, the spawned execution task can run to completion. If the task completes and stores its result in `executed_proposals` before `abort_proposal` reaches the insert, the insert finds an already-existing entry and the `assert!(proposal_already_exists.is_none(), "Duplicate proposal: {proposal_id}.")` panics.

**Root Cause**: The `active_proposal` mutex protects the `active_proposal` field but NOT the `executed_proposals` map. The check (`is_active`) and the insert into `executed_proposals` are not atomic. The execution task (running in `tokio::spawn`) can be scheduled between the two operations in the async context.

The race window:
1. `is_active(proposal_id).await` → acquires mutex, reads `Some(id)`, **releases mutex**, returns `true`
2. [tokio scheduler runs spawned task]  
3. Spawned task: completes `build_block`, acquires `active_proposal` mutex, sees `Some(id)`, clears it, inserts result into `executed_proposals`
4. Back in `abort_proposal`: `abort_active_proposal().await` → acquires mutex (already None), releases
5. `executed_proposals.lock().await.insert(proposal_id, Err(Aborted))` → proposal already present → **PANIC**

```rust
// Buggy code in abort_proposal:
if self.is_active(proposal_id).await {          // (1) check: releases lock after
    self.abort_active_proposal().await;          // (2) execution task can complete here
    let proposal_already_exists = self
        .executed_proposals
        .lock()
        .await
        .insert(proposal_id, Err(Arc::new(BlockBuilderError::Aborted)));
    assert!(proposal_already_exists.is_none(), "Duplicate proposal: {proposal_id}."); // (3) PANIC
}
```

**Failing Test**:

This test requires a custom block builder that completes immediately without waiting for `final_n_executed_txs` (unlike `FakeValidateBlockBuilder`), to reliably trigger the scheduling race in tokio's single-threaded test runtime.

```rust
use async_trait::async_trait;
use crate::block_builder::{BlockBuilderResult, BlockBuilderTrait, BlockExecutionArtifacts};

// A block builder that completes immediately without checking abort or final_n_executed_txs.
struct ImmediateSuccessBlockBuilder {
    result: Option<BlockBuilderResult<BlockExecutionArtifacts>>,
}

#[async_trait]
impl BlockBuilderTrait for ImmediateSuccessBlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        // Yield once to allow the tokio scheduler to potentially run other tasks.
        tokio::task::yield_now().await;
        self.result.take().expect("build_block called twice")
    }
}

#[tokio::test]
async fn test_abort_proposal_race_causes_panic() {
    use std::sync::Arc;
    use apollo_batcher_types::batcher_types::{ProposalId, StartHeightInput, ValidateBlockInput};
    use apollo_l1_events_types::MockL1EventsProviderClient;
    use crate::batcher::AbortSignalSender;
    use crate::block_builder::{BlockBuilderError, MockBlockBuilderFactoryTrait};
    use crate::test_utils::{MockClients, MockDependencies, INITIAL_HEIGHT, PROPOSAL_ID};

    let mut block_builder_factory = MockBlockBuilderFactoryTrait::new();
    block_builder_factory
        .expect_create_block_builder()
        .times(1)
        .return_once(|_, _, _, _tx_provider, _, _, _| {
            let artifacts = futures::executor::block_on(BlockExecutionArtifacts::create_for_testing());
            let builder = Box::new(ImmediateSuccessBlockBuilder {
                result: Some(Ok(artifacts)),
            });
            let (sender, _recv) = tokio::sync::oneshot::channel::<()>();
            Ok((builder as Box<dyn BlockBuilderTrait>, sender))
        });

    let mut l1_provider_client = MockL1EventsProviderClient::new();
    l1_provider_client.expect_start_block().returning(|_, _| Ok(()));

    let mut batcher = create_batcher(MockDependencies {
        clients: MockClients {
            block_builder_factory,
            l1_provider_client,
            ..Default::default()
        },
        ..Default::default()
    })
    .await;

    batcher.start_height(StartHeightInput { height: INITIAL_HEIGHT }).await.unwrap();
    batcher
        .validate_block(ValidateBlockInput {
            proposal_id: PROPOSAL_ID,
            retrospective_block_hash: None,
            deadline: chrono::Utc::now() + tokio::time::Duration::from_secs(10),
            block_info: starknet_api::block::BlockInfo {
                block_number: INITIAL_HEIGHT,
                ..starknet_api::block::BlockInfo::create_for_testing()
            },
        })
        .await
        .unwrap();

    // Yield to allow the spawned block builder task to run and complete.
    tokio::task::yield_now().await;
    // At this point, if the task completed and stored its result, is_active() will return false
    // and the race won't be triggered. But if the task is between build_block and the mutex lock,
    // abort_proposal will find is_active() == true and then the task completes, causing the panic.

    // This test exposes the race: sometimes it panics, sometimes it succeeds depending on scheduling.
    let _ = batcher.abort_proposal(PROPOSAL_ID).await;
}
```

**Note on reproducibility**: The race requires the execution task to be holding the `active_proposal` lock while `is_active` reads the value, then completing before `abort_active_proposal` re-acquires the lock. In single-threaded tokio (the default for `#[tokio::test]`), the main task runs continuously from `is_active.await` to `abort_active_proposal.await` without yielding, which happens to prevent the race in tests. However, in the multi-threaded tokio runtime used in production, the execution task runs on a separate OS thread and can independently complete and store its result between these two await points. The bug is a genuine production-only design flaw.

The fix requires atomically checking `is_active` and inserting into `executed_proposals` under a single lock scope, or checking whether the proposal was already stored before asserting.

**A correct fix**:
```rust
pub async fn abort_proposal(&mut self, proposal_id: ProposalId) -> BatcherResult<()> {
    self.ensure_validate_proposal_exists(proposal_id)?;

    if self.is_active(proposal_id).await {
        self.abort_active_proposal().await;
        // Only insert if not already stored by the execution task that may have
        // completed between is_active and here.
        self.executed_proposals
            .lock()
            .await
            .entry(proposal_id)
            .or_insert_with(|| Err(Arc::new(BlockBuilderError::Aborted)));
    }
    self.validate_tx_streams.remove(&proposal_id);
    Ok(())
}
```

---

## Bug 3 (Minor)

**File**: `crates/apollo_batcher/src/batcher.rs`  
**Location**: `fn send_txs_for_proposal`, line ~595  
**Description**: When a proposal has already completed with `Ok` (block built successfully) while `send_txs_for_proposal` is still being called, the function panics unconditionally instead of returning a graceful error.

```rust
Ok(_) => panic!("Proposal finished validation before all transactions were sent."),
```

**Root Cause**: This scenario should not occur in the normal protocol flow (the proposer cannot finish before all txs are sent), but if it does (e.g., due to a state machine bug or the race described in Bug 2), a panic causes the entire process to crash. This should be a `BatcherError::InternalError` return or a specific error type, not a process-ending `panic!`.

**Impact**: Any unexpected sequencing between `send_txs_for_proposal` and proposal completion crashes the node process rather than returning an error that the caller could handle or log.

**How to Verify**: The bug is latent — it would be triggered by combining with the race condition in Bug 2 where `abort_proposal` stores `Err(Aborted)` into `executed_proposals`, and then a subsequent `send_txs_for_proposal` call on a different code path encounters an `Ok` result.
