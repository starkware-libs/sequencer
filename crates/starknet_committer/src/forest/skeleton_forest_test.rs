use std::collections::HashMap;

use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::external_test_utils::{
    create_32_bytes_entry,
    create_binary_entry,
    create_binary_skeleton_node,
    create_edge_entry,
    create_edge_skeleton_node,
    create_expected_skeleton_nodes,
    create_root_edge_entry,
    create_unmodified_subtree_skeleton_node,
};
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices, SubTreeHeight};
use starknet_patricia_storage::db_object::DBObject;
use starknet_patricia_storage::map_storage::{BorrowedMapStorage, MapStorage};
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

use crate::block_committer::commit::get_all_modified_indices;
use crate::block_committer::input::{
    contract_address_into_node_index,
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

macro_rules! compare_skeleton_tree {
    ($actual_skeleton:expr, $expected_skeleton:expr, $expected_indices:expr) => {{
        let mut indices = create_expected_sorted_indices($expected_indices);
        let sorted_indices = SortedLeafIndices::new(&mut indices);
        let copied_expected_skeleton =
            create_original_skeleton_with_sorted_indices(sorted_indices, $expected_skeleton);
        assert_eq!($actual_skeleton, &copied_expected_skeleton);
    }};
}

pub(crate) fn create_storage_leaf_entry(val: u128) -> (DbKey, DbValue) {
    let leaf = StarknetStorageValue(Felt::from(val));
    (leaf.get_db_key(&leaf.0.to_bytes_be()), leaf.serialize())
}

pub(crate) fn create_compiled_class_leaf_entry(val: u128) -> (DbKey, DbValue) {
    let leaf = CompiledClassHash(Felt::from(val));
    (leaf.get_db_key(&leaf.0.to_bytes_be()), leaf.serialize())
}

pub(crate) fn create_contract_state_leaf_entry(val: u128) -> (DbKey, DbValue) {
    let felt = Felt::from(val);
    let leaf = ContractState {
        nonce: Nonce(felt),
        storage_root_hash: HashOutput(felt),
        class_hash: ClassHash(felt),
    };
    (leaf.get_db_key(&felt.to_bytes_be()), leaf.serialize())
}

// This test assumes for simplicity that hash is addition (i.e hash(a,b) = a + b).
// I.e., the value of a binary node is the sum of its children's values, and the value of an edge
// node is the sum of its path, bottom value and path length.
///                                Old forest structure:
///
///                      Global tree:                Classes tree:
///
///                   248 + 861 + 0                 248 + 155 + 0
///                         /                             /
///                       ...                           ...
///                       /                             /
///                      /                             /
///                     861                          155
///                    /   \                         /
///                   305  556                     154
///                  /       \                     /  \
///                 304      554                  80   74
///                /   \     /  \                /  \    \
///               303   1  277  277             33  47    72
///
/// Modified leaves (full) indices: [8, 14, 15]  ##  Modified leaves (full) indices: [8, 14, 15]
///
///
///         Contracts #6, #7:                                  Contract #0:
///
///             248 + 29 + 0                                248 + 55 + 0
///                   /                                           /
///                 ...                                         ...
///                 /                                           /
///                /                                           /
///               29                                          55
///             /    \                                      /    \
///           13      16                                  35      20
///          /      /    \                               /  \       \
///         12      5     11                            17  18       *
///        /  \      \   /  \                          /  \   \        \
///       10   2      3  4   7                        8    9  16       15
///
///   Modified leaves (full) indices: [8, 11, 13]  ##  Modified leaves (full) indices: [8, 10, 13]
///
///                             Expected skeleton forest:
///                 Global tree:                Classes tree:
///
///                    B                                 E
///                  /   \                              /
///                 E     E                            B
///                /       \                         /   \
///               /         B                       B     E
///              /                                 / \     \
///             303                               NZ  47   UB
///
///          Contracts #6, #7:                        Contract #0:
///
///
///              B                                           B
///            /   \                                       /   \
///          E      B                                     B     E
///         /     /    \                                 / \     \
///        B      E     E                               B   E     *
///       /  \     \     \                             / \   \     \
///      NZ   2     NZ    NZ                          NZ  9  16    15

#[rstest]
#[case(
    Input {
        state_diff: StateDiff {
            storage_updates: create_storage_updates(&[
                (7, &[0, 3, 5]),
                (6, &[0, 3, 5]),
                (0, &[0, 2, 5]),
            ]),
            class_hash_to_compiled_class_hash: create_class_hash_to_compiled_class_hash(&[(6, 1), (0, 7), (7, 9)]),
            ..Default::default()
        },
        contracts_trie_root_hash: HashOutput(Felt::from(861_u128 + 248_u128)),
        classes_trie_root_hash: HashOutput(Felt::from(155_u128 + 248_u128)),
        config: ConfigImpl::new(true, LevelFilter::DEBUG),
    },
    HashMap::from([
        // Roots.
        create_root_edge_entry(29, SubTreeHeight::new(3)),
        create_root_edge_entry(55, SubTreeHeight::new(3)),
        create_root_edge_entry(155, SubTreeHeight::new(3)),
        create_root_edge_entry(861, SubTreeHeight::new(3)),
        // Contracts trie inner nodes.
        create_binary_entry(303, 1),
        create_binary_entry(277, 277),
        create_edge_entry(304, 0, 1),
        create_edge_entry(554, 1, 1),
        create_binary_entry(305, 556),
        // Contracts trie leaves.
        create_contract_state_leaf_entry(277),
        create_contract_state_leaf_entry(303),
        create_contract_state_leaf_entry(1),
        // Classes trie inner nodes.
        create_binary_entry(33, 47),
        create_edge_entry(72, 1, 1),
        create_binary_entry(80, 74),
        create_edge_entry(154, 0, 1),
        // Classes trie leaves.
        create_compiled_class_leaf_entry(33),
        create_compiled_class_leaf_entry(47),
        create_compiled_class_leaf_entry(72),
        // Storage tries #6, #7 inner nodes.
        create_binary_entry(10, 2),
        create_edge_entry(3, 1, 1),
        create_binary_entry(4, 7),
        create_edge_entry(12, 0, 1),
        create_binary_entry(5, 11),
        create_binary_entry(13, 16),
        // Storage tries #6, #7 leaves.
        create_storage_leaf_entry(2),
        create_storage_leaf_entry(3),
        create_storage_leaf_entry(4),
        create_storage_leaf_entry(7),
        create_storage_leaf_entry(10),
        // Storage trie #0 inner nodes.
        create_binary_entry(8, 9),
        create_edge_entry(16, 1, 1),
        create_edge_entry(15, 3, 2),
        create_binary_entry(17, 18),
        create_binary_entry(35, 20),
        // Storage trie #0 leaves.
        create_storage_leaf_entry(8),
        create_storage_leaf_entry(9),
        create_storage_leaf_entry(15),
        create_storage_leaf_entry(16),
        ]),
     OriginalSkeletonForest{
        classes_trie: OriginalSkeletonTreeImpl {
            nodes: create_expected_skeleton_nodes(
                        vec![
                            create_edge_skeleton_node(1, 0, 1),
                            create_binary_skeleton_node(2),
                            create_binary_skeleton_node(4),
                            create_edge_skeleton_node(5, 1, 1),
                            create_unmodified_subtree_skeleton_node(11, 72),
                            create_unmodified_subtree_skeleton_node(9, 47)
                        ],
                        3
                    ),
            sorted_leaf_indices: SortedLeafIndices::new(&mut [])
        },
        contracts_trie: OriginalSkeletonTreeImpl {
            nodes: create_expected_skeleton_nodes(
                    vec![
                        create_binary_skeleton_node(1),
                        create_edge_skeleton_node(2, 0, 1),
                        create_binary_skeleton_node(4),
                        create_unmodified_subtree_skeleton_node(9, 1),
                        create_edge_skeleton_node(3, 1, 1),
                        create_binary_skeleton_node(7),
                    ],
                    3
                ),
            sorted_leaf_indices: SortedLeafIndices::new(&mut [])
        },
        storage_tries: HashMap::from([
            (
                ContractAddress::try_from(Felt::ZERO).unwrap(),
                OriginalSkeletonTreeImpl {
                    nodes: create_expected_skeleton_nodes(
                        vec![
                            create_binary_skeleton_node(1),
                            create_binary_skeleton_node(2),
                            create_edge_skeleton_node(3, 3, 2),
                            create_binary_skeleton_node(4),
                            create_edge_skeleton_node(5, 1, 1),
                            create_unmodified_subtree_skeleton_node(9, 9),
                            create_unmodified_subtree_skeleton_node(15, 15),
                            create_unmodified_subtree_skeleton_node(11, 16),
                        ],
                        3
                    ),
                    sorted_leaf_indices: SortedLeafIndices::new(&mut [])
                }
            ),
            (
                ContractAddress::try_from(Felt::from(6_u128)).unwrap(),
                OriginalSkeletonTreeImpl {
                    nodes: create_expected_skeleton_nodes(
                        vec![
                            create_binary_skeleton_node(1),
                            create_edge_skeleton_node(2, 0, 1),
                            create_binary_skeleton_node(3),
                            create_binary_skeleton_node(4),
                            create_edge_skeleton_node(6, 1, 1),
                            create_unmodified_subtree_skeleton_node(7, 11),
                            create_unmodified_subtree_skeleton_node(9, 2),
                        ],
                        3
                    ),
                    sorted_leaf_indices: SortedLeafIndices::new(&mut [])
                }
            ),
            (
                ContractAddress::try_from(Felt::from(7_u128)).unwrap(),
                OriginalSkeletonTreeImpl {
                    nodes: create_expected_skeleton_nodes(
                        vec![
                            create_binary_skeleton_node(1),
                            create_edge_skeleton_node(2, 0, 1),
                            create_binary_skeleton_node(3),
                            create_binary_skeleton_node(4),
                            create_edge_skeleton_node(6, 1, 1),
                            create_unmodified_subtree_skeleton_node(7, 11),
                            create_unmodified_subtree_skeleton_node(9, 2),
                        ],
                        3
                    ),
                    sorted_leaf_indices: SortedLeafIndices::new(&mut [])
                }
            )
            ]),
        },
        create_contract_leaves(&[
            (7, 29 + 248),
            (6, 29 + 248),
            (0, 55 + 248),
        ]),
        HashMap::from([(0, vec![2, 5, 0]), (6, vec![3, 5, 0]), (7, vec![5, 3, 0])]),
        vec![6, 7, 0],
        vec![7, 6, 0],
)]
fn test_create_original_skeleton_forest(
    #[case] input: Input<ConfigImpl>,
    #[case] mut storage: MapStorage,
    #[case] expected_forest: OriginalSkeletonForest<'_>,
    #[case] expected_original_contracts_trie_leaves: HashMap<ContractAddress, ContractState>,
    #[case] expected_storage_tries_sorted_indices: HashMap<u128, Vec<u128>>,
    #[case] expected_contracts_trie_sorted_indices: Vec<u128>,
    #[case] expected_classes_trie_sorted_indices: Vec<u128>,
) {
    let (mut storage_tries_indices, mut contracts_trie_indices, mut classes_trie_indices) =
        get_all_modified_indices(&input.state_diff);
    let forest_sorted_indices = ForestSortedIndices {
        storage_tries_sorted_indices: storage_tries_indices
            .iter_mut()
            .map(|(address, indices)| (*address, SortedLeafIndices::new(indices)))
            .collect(),
        contracts_trie_sorted_indices: SortedLeafIndices::new(&mut contracts_trie_indices),
        classes_trie_sorted_indices: SortedLeafIndices::new(&mut classes_trie_indices),
    };

    let (actual_forest, original_contracts_trie_leaves) = OriginalSkeletonForest::create(
        BorrowedMapStorage { storage: &mut storage },
        input.contracts_trie_root_hash,
        input.classes_trie_root_hash,
        &input.state_diff.actual_storage_updates(),
        &input.state_diff.actual_classes_updates(),
        &forest_sorted_indices,
        &ConfigImpl::new(false, LevelFilter::DEBUG),
    )
    .unwrap();
    let expected_original_contracts_trie_leaves = expected_original_contracts_trie_leaves
        .into_iter()
        .map(|(address, state)| (contract_address_into_node_index(&address), state))
        .collect();
    assert_eq!(original_contracts_trie_leaves, expected_original_contracts_trie_leaves);

    compare_skeleton_tree!(
        &actual_forest.classes_trie,
        &expected_forest.classes_trie,
        &expected_classes_trie_sorted_indices
    );

    compare_skeleton_tree!(
        &actual_forest.contracts_trie,
        &expected_forest.contracts_trie,
        &expected_contracts_trie_sorted_indices
    );

    for (contract, indices) in expected_storage_tries_sorted_indices {
        let contract_address = ContractAddress::try_from(Felt::from(contract)).unwrap();
        compare_skeleton_tree!(
            &actual_forest.storage_tries[&contract_address],
            &expected_forest.storage_tries[&contract_address],
            &indices
        );
    }
}

fn create_contract_leaves(leaves: &[(u128, u128)]) -> HashMap<ContractAddress, ContractState> {
    leaves
        .iter()
        .map(|(idx, root)| {
            (
                ContractAddress::try_from(Felt::from_bytes_be_slice(&create_32_bytes_entry(*idx)))
                    .unwrap(),
                ContractState {
                    nonce: Nonce(Felt::from(*root)),
                    storage_root_hash: HashOutput(Felt::from(*root)),
                    class_hash: ClassHash(Felt::from(*root)),
                },
            )
        })
        .collect()
}

fn create_storage_updates(
    updates: &[(u8, &[u8])],
) -> HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>> {
    updates
        .iter()
        .map(|(address, address_indices)| {
            (
                ContractAddress::try_from(Felt::from(u128::from(*address))).unwrap(),
                address_indices
                    .iter()
                    .map(|val| {
                        (
                            StarknetStorageKey(StorageKey::from(u128::from(*val))),
                            StarknetStorageValue(Felt::from(u128::from(*val))),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

fn create_class_hash_to_compiled_class_hash(
    map: &[(u128, u128)],
) -> HashMap<ClassHash, CompiledClassHash> {
    map.iter()
        .map(|(class_hash, compiled_class_hash)| {
            (
                ClassHash(Felt::from(*class_hash)),
                CompiledClassHash(Felt::from(*compiled_class_hash)),
            )
        })
        .collect()
}

fn create_original_skeleton_with_sorted_indices<'a>(
    indices: SortedLeafIndices<'a>,
    skeleton: &OriginalSkeletonTreeImpl<'_>,
) -> OriginalSkeletonTreeImpl<'a> {
    OriginalSkeletonTreeImpl { nodes: skeleton.nodes.clone(), sorted_leaf_indices: indices }
}

fn create_expected_sorted_indices(indices: &[u128]) -> Vec<NodeIndex> {
    indices.iter().map(|idx| NodeIndex::FIRST_LEAF + NodeIndex::from(*idx)).collect()
}
