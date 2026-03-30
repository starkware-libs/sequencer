use std::collections::HashMap;

use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash as ApiCompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_committer::block_committer::input::{
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash as CommitterCompiledClassHash;
use starknet_types_core::felt::Felt;

use super::{compute_actual_end, shrink_to_actual_end, trailing_zeros, TreeKey, TreeRequest};

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

fn ch(n: u64) -> ClassHash {
    ClassHash(Felt::from(n))
}

fn addr(n: u64) -> ContractAddress {
    ContractAddress(PatriciaKey::try_from(Felt::from(n)).unwrap())
}

fn storage_key(n: u64) -> StorageKey {
    StorageKey(PatriciaKey::try_from(Felt::from(n)).unwrap())
}

fn compiled_ch(n: u64) -> ApiCompiledClassHash {
    ApiCompiledClassHash(Felt::from(n))
}

fn nonce(n: u64) -> Nonce {
    Nonce(Felt::from(n))
}

#[test]
fn test_trailing_zeros_powers_of_two() {
    assert_eq!(trailing_zeros(Felt::ONE), 0);
    assert_eq!(trailing_zeros(Felt::from(2u64)), 1);
    assert_eq!(trailing_zeros(Felt::from(4u64)), 2);
    assert_eq!(trailing_zeros(Felt::from(256u64)), 8);
}

#[test]
fn test_trailing_zeros_non_powers_of_two() {
    assert_eq!(trailing_zeros(Felt::from(3u64)), 0);
    assert_eq!(trailing_zeros(Felt::from(6u64)), 1);
    assert_eq!(trailing_zeros(Felt::from(12u64)), 2);
    assert_eq!(trailing_zeros(Felt::from(24u64)), 3);
    assert_eq!(trailing_zeros(Felt::from(255u64)), 0);
}

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
    let entries = vec![(ch(0), ()), (ch(1), ()), (ch(2), ())];
    let (result, actual_end) = shrink_to_actual_end(entries.clone(), ch(0), ch(16), 4);
    assert_eq!(result, entries);
    assert_eq!(actual_end, Felt::from(16u64));
}

#[test]
fn test_shrink_to_actual_end_at_limit_truncates() {
    // start=0, last_key=4 → covered=5, subtree_size=4, actual_end=3 (inclusive); entry at key 4
    // is dropped.
    let entries = vec![(ch(0), ()), (ch(1), ()), (ch(2), ()), (ch(4), ())];
    let (result, actual_end) = shrink_to_actual_end(entries, ch(0), ch(8), 4);
    assert_eq!(result, vec![(ch(0), ()), (ch(1), ()), (ch(2), ())]);
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

    let request = TreeRequest {
        context,
        start: K::from_felt(Felt::ZERO),
        end: K::from_felt(Felt::from(255)),
    };
    let (state_diff, actual_end) = K::scan(&reader, &request, BlockNumber(0), size_limit);
    // With size_limit=1 the last entry is ENTRY_KEY_1=1:
    // shrink_to_actual_end snaps to the subtree [0, 1], so actual_end=1.
    assert_eq!(actual_end, Felt::ONE);
    assert_eq!(state_diff, expected_state_diff);
}

#[test]
fn test_class_hash_scan() {
    run_scan_testing::<ClassHash>(
        ThinStateDiff {
            class_hash_to_compiled_class_hash: BLOCK_0_ENTRIES
                .into_iter()
                .map(|(key, value)| (ch(key), compiled_ch(value)))
                .collect(),
            ..Default::default()
        },
        // Block 1 updates ch(ENTRY_KEY_1); scanning at block 0 must return the original value.
        ThinStateDiff {
            class_hash_to_compiled_class_hash: BLOCK_1_ENTRIES
                .into_iter()
                .map(|(key, value)| (ch(key), compiled_ch(value)))
                .collect(),
            ..Default::default()
        },
        (),
        1,
        StateDiff {
            class_hash_to_compiled_class_hash: EXPECTED_SCANNED_ENTRIES
                .into_iter()
                .map(|(key, value)| (ch(key), CommitterCompiledClassHash(Felt::from(value))))
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
                .map(|(key, value)| (addr(key), ch(value)))
                .collect(),
            nonces: BLOCK_0_ENTRIES
                .into_iter()
                .map(|(key, value)| (addr(key), nonce(value)))
                .collect(),
            ..Default::default()
        },
        ThinStateDiff {
            deployed_contracts: BLOCK_1_ENTRIES
                .into_iter()
                .map(|(key, value)| (addr(key), ch(value)))
                .collect(),
            nonces: BLOCK_1_ENTRIES
                .into_iter()
                .map(|(key, value)| (addr(key), nonce(value)))
                .collect(),
            ..Default::default()
        },
        (),
        // size_limit=2 implies internal class hash limit of 1.
        2,
        StateDiff {
            address_to_class_hash: EXPECTED_SCANNED_ENTRIES
                .into_iter()
                .map(|(key, value)| (addr(key), ch(value)))
                .collect(),
            address_to_nonce: EXPECTED_SCANNED_ENTRIES
                .into_iter()
                .map(|(key, value)| (addr(key), nonce(value)))
                .collect(),
            ..Default::default()
        },
    );
}

#[test]
fn test_storage_key_scan() {
    let contract = addr(1);
    run_scan_testing::<StorageKey>(
        ThinStateDiff {
            storage_diffs: IndexMap::from([(
                contract,
                BLOCK_0_ENTRIES
                    .into_iter()
                    .map(|(key, value)| (storage_key(key), Felt::from(value)))
                    .collect(),
            )]),
            ..Default::default()
        },
        ThinStateDiff {
            storage_diffs: IndexMap::from([(
                contract,
                BLOCK_1_ENTRIES
                    .into_iter()
                    .map(|(key, value)| (storage_key(key), Felt::from(value)))
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
                            StarknetStorageKey(storage_key(key)),
                            StarknetStorageValue(Felt::from(value)),
                        )
                    })
                    .collect(),
            )]),
            ..Default::default()
        },
    );
}
