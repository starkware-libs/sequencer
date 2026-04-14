use std::collections::HashMap;
use std::sync::Arc;

use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::StateRoots;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::{
    class_hash,
    compiled_class_hash,
    contract_address,
    nonce,
    patricia_key,
    storage_key,
};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_committer::db::index_db::IndexDb;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash as CommitterCompiledClassHash;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;
use tokio::sync::Mutex;

use super::{
    compute_actual_end,
    process_request,
    shrink_to_actual_end,
    CommitState,
    TreeKey,
    TreeRequest,
};

// Key positions and values shared across all scan tests.
const ENTRY_KEY_1: u64 = 1;
const ENTRY_KEY_2: u64 = 2;
const ENTRY_VALUE_1_BLOCK_0: u64 = 10;
const ENTRY_VALUE_1_BLOCK_1: u64 = 99;
const ENTRY_VALUE_2: u64 = 20;

const BLOCK_0_ENTRIES: [(u64, u64); 2] =
    [(ENTRY_KEY_1, ENTRY_VALUE_1_BLOCK_0), (ENTRY_KEY_2, ENTRY_VALUE_2)];
const BLOCK_1_ENTRIES: [(u64, u64); 1] = [(ENTRY_KEY_1, ENTRY_VALUE_1_BLOCK_1)];
// size_limit=1 at block 0: only ENTRY_KEY_1 returned.
const EXPECTED_SCANNED_ENTRIES: [(u64, u64); 1] = [(ENTRY_KEY_1, ENTRY_VALUE_1_BLOCK_0)];
// With size_limit=1 the last entry is ENTRY_KEY_1=1:
// shrink_to_actual_end snaps to the subtree [0, 1], so actual_end=1.
const EXPECTED_ACTUAL_END: Felt = Felt::ONE;

#[test]
fn test_compute_actual_end_single_element() {
    // start == last_key: covered=1, subtree_size=1, actual_end=start
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::ZERO), Felt::ZERO);
    assert_eq!(compute_actual_end(Felt::from(5u64), Felt::from(5u64)), Felt::from(5u64));
}

#[test]
fn test_compute_actual_end_covered_is_exact_power_of_two() {
    // start=0, last_key=3: covered=4, subtree_size=4, actual_end=3
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(3u64)), Felt::from(3u64));
    // start=0, last_key=7: covered=8, subtree_size=8, actual_end=7
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(7u64)), Felt::from(7u64));
}

#[test]
fn test_compute_actual_end_covered_is_not_power_of_two() {
    // start=0, last_key=4: covered=5, subtree_size=4, actual_end=3
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(4u64)), Felt::from(3u64));
    // start=0, last_key=6: covered=7, subtree_size=4, actual_end=3
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(6u64)), Felt::from(3u64));
    // start=0, last_key=14: covered=15, subtree_size=8, actual_end=7
    assert_eq!(compute_actual_end(Felt::ZERO, Felt::from(14u64)), Felt::from(7u64));
}

#[test]
fn test_compute_actual_end_non_zero_start() {
    // start=8, last_key=12: covered=5, subtree_size=4, actual_end=8+4-1=11
    assert_eq!(compute_actual_end(Felt::from(8u64), Felt::from(12u64)), Felt::from(11u64));
    // start=8, last_key=15: covered=8, subtree_size=8, actual_end=8+8-1=15
    assert_eq!(compute_actual_end(Felt::from(8u64), Felt::from(15u64)), Felt::from(15u64));
    // start=8, last_key=16: covered=9, subtree_size=8, actual_end=8+8-1=15
    assert_eq!(compute_actual_end(Felt::from(8u64), Felt::from(16u64)), Felt::from(15u64));
}

#[test]
fn test_compute_actual_end_unaligned_start() {
    // Alignment of 12 is 4, so the actual end is 12 + 4 - 1 = 15.
    assert_eq!(compute_actual_end(Felt::from(12u64), Felt::from(31u64)), Felt::from(15u64));
    // Alignment of 6 is 2, so the actual end is 6 + 2 - 1 = 7.
    assert_eq!(compute_actual_end(Felt::from(6u64), Felt::from(15u64)), Felt::from(7u64));
    // Alignment of 12 is 4, but the last key = 14 < 12 + 4 - 1 = 15.
    // So the actual end is determined by the last key.
    assert_eq!(compute_actual_end(Felt::from(12u64), Felt::from(14u64)), Felt::from(13u64));
}

#[test]
fn test_shrink_to_actual_end_fewer_than_limit() {
    // Under the limit: all entries returned, end returned as-is.
    let entries =
        vec![(class_hash!(0_u64), ()), (class_hash!(1_u64), ()), (class_hash!(2_u64), ())];
    let end: u64 = 16;
    let (result, actual_end) =
        shrink_to_actual_end(entries.clone(), patricia_key!(0_u64), patricia_key!(end), 4).unwrap();
    assert_eq!(result, entries);
    assert_eq!(actual_end, Felt::from(end));
}

#[test]
fn test_shrink_to_actual_end_at_limit_truncates() {
    // start=0, last_key=4 → covered=5, subtree_size=4, actual_end=3 (inclusive); entry at key 4
    // is dropped.
    let entries = vec![
        (class_hash!(0_u64), ()),
        (class_hash!(1_u64), ()),
        (class_hash!(2_u64), ()),
        (class_hash!(4_u64), ()),
    ];
    let (result, actual_end) =
        shrink_to_actual_end(entries, patricia_key!(0_u64), patricia_key!(8_u64), 4).unwrap();
    assert_eq!(
        result,
        vec![(class_hash!(0_u64), ()), (class_hash!(1_u64), ()), (class_hash!(2_u64), ()),]
    );
    assert_eq!(actual_end, Felt::from(3u64));
}

/// Sets up storage with two blocks, runs a scan at block 0 and asserts both `actual_end` and the
/// returned `StateDiff`.
fn run_scan_testing<K: TreeKey>(
    diff_at_block_0: ThinStateDiff,
    diff_at_block_1: ThinStateDiff,
    context: K::Context,
    size_limit: usize,
    expected_state_diff: StateDiff,
) {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff_at_block_0).unwrap();
    txn = txn.append_state_diff(BlockNumber(1), diff_at_block_1).unwrap();
    txn.commit().unwrap();

    let request = TreeRequest { context, start: patricia_key!(0_u64), end: patricia_key!(255_u64) };
    let (state_diff, actual_end) = K::scan(&reader, &request, BlockNumber(0), size_limit).unwrap();
    assert_eq!(actual_end, EXPECTED_ACTUAL_END);
    assert_eq!(state_diff, expected_state_diff);
}

#[test]
fn test_class_hash_scan() {
    run_scan_testing::<ClassHash>(
        ThinStateDiff {
            class_hash_to_compiled_class_hash: BLOCK_0_ENTRIES
                .into_iter()
                .map(|(key, value)| (class_hash!(key), compiled_class_hash!(value)))
                .collect(),
            ..Default::default()
        },
        // Block 1 updates class_hash!(ENTRY_KEY_1); scanning at block 0 must return the original
        // value.
        ThinStateDiff {
            class_hash_to_compiled_class_hash: BLOCK_1_ENTRIES
                .into_iter()
                .map(|(key, value)| (class_hash!(key), compiled_class_hash!(value)))
                .collect(),
            ..Default::default()
        },
        (),
        1,
        StateDiff {
            class_hash_to_compiled_class_hash: EXPECTED_SCANNED_ENTRIES
                .into_iter()
                .map(|(key, value)| {
                    (class_hash!(key), CommitterCompiledClassHash(Felt::from(value)))
                })
                .collect(),
            ..Default::default()
        },
    );
}

#[test]
fn test_contract_address_scan() {
    run_scan_testing::<ContractAddress>(
        ThinStateDiff {
            deployed_contracts: BLOCK_0_ENTRIES
                .into_iter()
                .map(|(key, value)| (contract_address!(key), class_hash!(value)))
                .collect(),
            nonces: BLOCK_0_ENTRIES
                .into_iter()
                .map(|(key, value)| (contract_address!(key), nonce!(value)))
                .collect(),
            ..Default::default()
        },
        ThinStateDiff {
            deployed_contracts: BLOCK_1_ENTRIES
                .into_iter()
                .map(|(key, value)| (contract_address!(key), class_hash!(value)))
                .collect(),
            nonces: BLOCK_1_ENTRIES
                .into_iter()
                .map(|(key, value)| (contract_address!(key), nonce!(value)))
                .collect(),
            ..Default::default()
        },
        (),
        // size_limit=2 implies internal class hash limit of 1.
        2,
        StateDiff {
            address_to_class_hash: EXPECTED_SCANNED_ENTRIES
                .into_iter()
                .map(|(key, value)| (contract_address!(key), class_hash!(value)))
                .collect(),
            address_to_nonce: EXPECTED_SCANNED_ENTRIES
                .into_iter()
                .map(|(key, value)| (contract_address!(key), nonce!(value)))
                .collect(),
            ..Default::default()
        },
    );
}

#[test]
fn test_storage_key_scan() {
    let contract = contract_address!(1_u64);
    run_scan_testing::<StorageKey>(
        ThinStateDiff {
            storage_diffs: IndexMap::from([(
                contract,
                BLOCK_0_ENTRIES
                    .into_iter()
                    .map(|(key, value)| (storage_key!(key), Felt::from(value)))
                    .collect(),
            )]),
            ..Default::default()
        },
        ThinStateDiff {
            storage_diffs: IndexMap::from([(
                contract,
                BLOCK_1_ENTRIES
                    .into_iter()
                    .map(|(key, value)| (storage_key!(key), Felt::from(value)))
                    .collect(),
            )]),
            ..Default::default()
        },
        contract,
        1,
        StateDiff {
            storage_updates: HashMap::from([(
                contract,
                EXPECTED_SCANNED_ENTRIES
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            StarknetStorageKey(storage_key!(key)),
                            StarknetStorageValue(Felt::from(value)),
                        )
                    })
                    .collect(),
            )]),
            ..Default::default()
        },
    );
}

// --- process_request tests ---

fn create_test_commit_state() -> Arc<Mutex<CommitState<MapStorage>>> {
    Arc::new(Mutex::new(CommitState {
        committer_db: IndexDb::new(MapStorage::default()),
        state_roots: StateRoots::default(),
        num_commits: 0,
    }))
}

/// Sets up storage with `diff` at block 0, runs `process_request` for class hashes over the
/// full key range with the given `size_limit`, and returns the resulting `StateRoots` and
/// the total number of commits made.
async fn run_process_request_for_class_hashes(
    diff: ThinStateDiff,
    size_limit: usize,
) -> (StateRoots, usize) {
    let ((reader, mut writer), _temp_dir_storage) = get_test_storage();
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_state_diff(BlockNumber(0), diff).unwrap();
    txn.commit().unwrap();
    let reader = Arc::new(reader);
    let commit_state = create_test_commit_state();
    process_request(
        reader,
        ClassHash::initial_request(()),
        BlockNumber(0),
        size_limit,
        Arc::clone(&commit_state),
    )
    .await;
    let final_state = Arc::try_unwrap(commit_state).ok().unwrap().into_inner();
    (final_state.state_roots, final_state.num_commits)
}

/// With no entries in storage, process_request makes no commits and state_roots remain default.
#[tokio::test]
async fn test_process_request_empty_storage() {
    let (state_roots, num_commits) =
        run_process_request_for_class_hashes(ThinStateDiff::default(), 100).await;
    assert_eq!(state_roots, StateRoots::default());
    assert_eq!(num_commits, 0);
}

/// With entries present, process_request commits them and state_roots diverge from default.
#[tokio::test]
async fn test_process_request_commits_entries() {
    let diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: [
            (class_hash!(1_u64), compiled_class_hash!(10_u64)),
            (class_hash!(2_u64), compiled_class_hash!(20_u64)),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let (state_roots, num_commits) = run_process_request_for_class_hashes(diff, 100).await;
    assert_ne!(state_roots, StateRoots::default());
    assert_eq!(num_commits, 1);
}

/// Validates that the global root after processing all entries with a large size_limit
/// (single scan, one commit) equals the global root after processing with a small size_limit
/// (multiple partial scans, multiple commits).
#[tokio::test]
async fn test_process_request_global_root_equals_accumulated_partial_roots() {
    let diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: [
            (class_hash!(1_u64), compiled_class_hash!(10_u64)),
            (class_hash!(2_u64), compiled_class_hash!(20_u64)),
            (class_hash!(5_u64), compiled_class_hash!(50_u64)),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let (root_single_scan, num_commits_single) =
        run_process_request_for_class_hashes(diff.clone(), 100).await;
    let (root_partial_scans, num_commits_partial) =
        run_process_request_for_class_hashes(diff, 1).await;
    // Single scan commits all three entries at once; partial scans commit one at a time.
    assert_eq!(num_commits_single, 1);
    assert_eq!(num_commits_partial, 3);
    assert_eq!(root_single_scan, root_partial_scans);
}
