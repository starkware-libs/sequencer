use std::collections::{BTreeSet, HashMap};

use apollo_committer_types::committer_types::{
    AccessedKeys,
    CommitBlockRequest,
    ReadPathsAndCommitBlockRequest,
    RevertBlockRequest,
};
use indexmap::indexmap;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, StateDiffCommitment};
use starknet_api::hash::HashOutput;
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::input::{
    contract_address_into_node_index,
    StarknetStorageKey,
    StarknetStorageValue,
};
use starknet_committer::db::facts_db::db::{FactDbFilledNode, FactsNodeLayout};
use starknet_committer::db::forest_trait::forest_trait_witnesses::ForestReaderWithWitnesses;
use starknet_committer::db::forest_trait::{EmptyInitialReadContext, ForestReader};
use starknet_committer::db::index_db::IndexDbReadContext;
use starknet_committer::db::serde_db_utils::accessed_keys_digest;
use starknet_committer::db::trie_traversal::fetch_patricia_paths;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::{LeavesRequest, SortedLeavesRequest};
use starknet_committer::patricia_merkle_tree::types::{
    class_hash_into_node_index,
    CompiledClassHash as CommitterCompiledClassHash,
    StarknetForestProofs,
};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::DbHashMap;

use crate::committer::committer_test::{new_test_committer, ApolloTestCommitter};

const ACCESSED_CLASS_HASH: u64 = 1;
const ACCESSED_CONTRACT_1: u128 = 1;
const ACCESSED_CONTRACT_2: u128 = 2;
const ACCESSED_STORAGE_KEY_1: u128 = 10;
const ACCESSED_STORAGE_KEY_2: u128 = 20;
const ACCESSED_STORAGE_VALUE_1: u128 = 100;
const ACCESSED_STORAGE_VALUE_2: u128 = 200;
const UNACCESSED_CLASS_HASH: u64 = 2;
const UNACCESSED_CONTRACT: u128 = 3;
const UNACCESSED_STORAGE_KEY: u128 = 11;
const UNACCESSED_STORAGE_VALUE: u128 = 300;

fn accessed_class_hash() -> ClassHash {
    ClassHash(ACCESSED_CLASS_HASH.into())
}

fn accessed_contract_1() -> ContractAddress {
    ContractAddress::from(ACCESSED_CONTRACT_1)
}

fn accessed_contract_2() -> ContractAddress {
    ContractAddress::from(ACCESSED_CONTRACT_2)
}

fn accessed_storage_key_1() -> StarknetStorageKey {
    StarknetStorageKey::from(ACCESSED_STORAGE_KEY_1)
}

fn accessed_storage_key_2() -> StarknetStorageKey {
    StarknetStorageKey::from(ACCESSED_STORAGE_KEY_2)
}

fn accessed_keys() -> AccessedKeys {
    AccessedKeys {
        accessed_class_hashes: BTreeSet::from([accessed_class_hash()]),
        accessed_contracts: BTreeSet::from([accessed_contract_1(), accessed_contract_2()]),
        storage_keys: BTreeSet::from([
            (accessed_contract_1(), accessed_storage_key_1().0),
            (accessed_contract_2(), accessed_storage_key_2().0),
        ]),
    }
}

fn contract_storage_keys(
    accessed_keys: &AccessedKeys,
) -> HashMap<ContractAddress, Vec<StarknetStorageKey>> {
    accessed_keys.storage_keys.iter().fold(HashMap::new(), |mut accumulator, (address, key)| {
        accumulator.entry(*address).or_default().push(StarknetStorageKey(*key));
        accumulator
    })
}

fn expected_accessed_keys_digest(accessed_keys: &AccessedKeys) -> [u8; 32] {
    let class_hashes: Vec<_> = accessed_keys.accessed_class_hashes.iter().copied().collect();
    let contract_addresses: Vec<_> = accessed_keys.accessed_contracts.iter().copied().collect();
    let contract_storage_keys = contract_storage_keys(accessed_keys);
    let mut leaves_request = LeavesRequest::from_accessed_leaves(
        &class_hashes,
        &contract_addresses,
        &contract_storage_keys,
    );
    let sorted_leaves = SortedLeavesRequest::from(&mut leaves_request);
    accessed_keys_digest(&sorted_leaves)
}

/// Leaf values for accessed class indices. Required to build facts-layout storage from the
/// returned [`PreimageMap`], which contains inner nodes only.
fn accessed_class_leaves() -> HashMap<ClassHash, CommitterCompiledClassHash> {
    HashMap::from([(accessed_class_hash(), CommitterCompiledClassHash(ACCESSED_CLASS_HASH.into()))])
}

/// Leaf values for accessed storage indices, per contract. Required to build facts-layout
/// storage from the returned [`PreimageMap`], which contains inner nodes only.
fn accessed_storage_leaves()
-> HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>> {
    HashMap::from([
        (
            accessed_contract_1(),
            HashMap::from([(
                accessed_storage_key_1(),
                StarknetStorageValue(ACCESSED_STORAGE_VALUE_1.into()),
            )]),
        ),
        (
            accessed_contract_2(),
            HashMap::from([(
                accessed_storage_key_2(),
                StarknetStorageValue(ACCESSED_STORAGE_VALUE_2.into()),
            )]),
        ),
    ])
}

fn block_0_state_diff() -> ThinStateDiff {
    let class_hash = accessed_class_hash();
    let unaccessed_class_hash = ClassHash(UNACCESSED_CLASS_HASH.into());
    let contract_1 = accessed_contract_1();
    let contract_2 = accessed_contract_2();
    let unaccessed_contract = ContractAddress::from(UNACCESSED_CONTRACT);

    ThinStateDiff {
        deployed_contracts: indexmap! {
            contract_1 => class_hash,
            contract_2 => class_hash,
            unaccessed_contract => class_hash,
        },
        storage_diffs: indexmap! {
            contract_1 => indexmap! {
                accessed_storage_key_1().0 => ACCESSED_STORAGE_VALUE_1.into(),
                UNACCESSED_STORAGE_KEY.into() => UNACCESSED_STORAGE_VALUE.into(),
            },
            contract_2 => indexmap! {
                accessed_storage_key_2().0 => ACCESSED_STORAGE_VALUE_2.into(),
            },
        },
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash => CompiledClassHash(ACCESSED_CLASS_HASH.into()),
            unaccessed_class_hash => CompiledClassHash(UNACCESSED_CLASS_HASH.into()),
        },
        ..Default::default()
    }
}

fn block_1_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        storage_diffs: indexmap! {
            accessed_contract_1() => indexmap! {
                accessed_storage_key_1().0 => 101_u128.into(),
                UNACCESSED_STORAGE_KEY.into() => 301_u128.into(),
            },
        },
        ..Default::default()
    }
}

fn block_1_reversed_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        storage_diffs: indexmap! {
            accessed_contract_1() => indexmap! {
                accessed_storage_key_1().0 => ACCESSED_STORAGE_VALUE_1.into(),
                UNACCESSED_STORAGE_KEY.into() => UNACCESSED_STORAGE_VALUE.into(),
            },
        },
        ..Default::default()
    }
}

fn read_paths_and_commit_block_request(
    state_diff: ThinStateDiff,
    state_diff_commitment: Option<StateDiffCommitment>,
    height: u64,
    accessed_keys: AccessedKeys,
) -> ReadPathsAndCommitBlockRequest {
    ReadPathsAndCommitBlockRequest {
        commit: CommitBlockRequest {
            state_diff,
            state_diff_commitment,
            height: BlockNumber(height),
        },
        accessed_keys,
    }
}

/// Flow overview:
/// 1. Commit block 0 via [crate::committer::Committer::read_paths_and_commit_block], requesting
///    witnesses for [accessed_keys].
/// 2. Verify the returned Patricia proofs via [verify_witness_patricia_paths].
/// 3. Clear trie storage and replay the same request to verify witnesses are loaded from storage
///    rather than recomputed.
/// 4. Assert witnesses and the accessed-keys digest are stored for block 0 via
///    [assert_witnesses_and_digest_present].
#[tokio::test]
async fn read_paths_and_commit_block_happy_flow() {
    let mut committer = new_test_committer().await;
    let height = 0;
    let state_diff = block_0_state_diff();
    let state_diff_commitment = Some(calculate_state_diff_hash(&state_diff));
    let accessed_keys = accessed_keys();
    let request = read_paths_and_commit_block_request(
        state_diff,
        state_diff_commitment,
        height,
        accessed_keys.clone(),
    );

    let response = committer.read_paths_and_commit_block(request.clone()).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(height + 1));
    let roots =
        committer.forest_storage.read_roots(IndexDbReadContext::create_empty()).await.unwrap();
    verify_witness_patricia_paths(
        &response.patricia_proofs,
        &accessed_keys,
        &accessed_class_leaves(),
        &accessed_storage_leaves(),
        roots.classes_trie_root_hash,
        roots.contracts_trie_root_hash,
    )
    .await;

    // Historical replay should load persisted witnesses, removing trie nodes to assert this.
    committer.forest_storage.clear_patricia_trie_nodes_for_test();

    let replay_response = committer.read_paths_and_commit_block(request).await.unwrap();
    assert_eq!(response.global_root, replay_response.global_root);
    assert_eq!(response.patricia_proofs, replay_response.patricia_proofs);
    assert_witnesses_and_digest_present(
        &mut committer,
        BlockNumber(height),
        &accessed_keys,
        &response.patricia_proofs,
    )
    .await;
}

/// Flow overview:
/// 1. Commit block 0 via [crate::committer::Committer::commit_block] (no witnesses fetched).
/// 2. Commit block 1 via [crate::committer::Committer::read_paths_and_commit_block], requesting
///    witnesses for [accessed_keys].
/// 3. Assert witnesses and the accessed-keys digest are present for block 1 via
///    [assert_witnesses_and_digest_present].
/// 4. Revert block 1 via [crate::committer::Committer::revert_block].
/// 5. Assert witnesses and the accessed-keys digest are absent for block 1 via
///    [assert_witnesses_and_digest_absent].
#[tokio::test]
async fn revert_removes_witnesses_and_digest() {
    let mut committer = new_test_committer().await;
    let height_0 = 0;
    let height_1 = 1;
    let block_0_state_diff = block_0_state_diff();
    let block_1_state_diff = block_1_state_diff();
    let accessed_keys = accessed_keys();

    committer
        .commit_block(CommitBlockRequest {
            state_diff: block_0_state_diff.clone(),
            state_diff_commitment: Some(calculate_state_diff_hash(&block_0_state_diff)),
            height: BlockNumber(height_0),
        })
        .await
        .unwrap();

    let block_1_response = committer
        .read_paths_and_commit_block(read_paths_and_commit_block_request(
            block_1_state_diff.clone(),
            Some(calculate_state_diff_hash(&block_1_state_diff)),
            height_1,
            accessed_keys.clone(),
        ))
        .await
        .unwrap();
    assert_witnesses_and_digest_present(
        &mut committer,
        BlockNumber(height_1),
        &accessed_keys,
        &block_1_response.patricia_proofs,
    )
    .await;

    committer
        .revert_block(RevertBlockRequest {
            reversed_state_diff: block_1_reversed_state_diff(),
            height: BlockNumber(height_1),
        })
        .await
        .unwrap();
    assert_witnesses_and_digest_absent(&mut committer, BlockNumber(height_1)).await;
    assert_eq!(committer.offset, BlockNumber(height_1));
}

fn fact_db_node_from_preimage<L: Leaf>(
    hash: HashOutput,
    preimage: &Preimage,
) -> FactDbFilledNode<L> {
    let data = match preimage {
        Preimage::Binary(BinaryData { left_data, right_data }) => {
            NodeData::Binary(BinaryData { left_data: *left_data, right_data: *right_data })
        }
        Preimage::Edge(EdgeData { bottom_data, path_to_bottom }) => {
            NodeData::Edge(EdgeData { bottom_data: *bottom_data, path_to_bottom: *path_to_bottom })
        }
    };
    FactDbFilledNode(FilledNode { hash, data })
}

fn insert_filled_node<L: Leaf + DBObject>(
    storage: &mut DbHashMap,
    key_context: &<L as HasStaticPrefix>::KeyContext,
    filled_node: FilledNode<L, HashOutput>,
) {
    let db_object = FactDbFilledNode(filled_node);
    let key = db_object.get_db_key(key_context, &db_object.0.hash.0.to_bytes_be());
    storage.insert(key, db_object.serialize().expect("facts node serialization should succeed"));
}

fn preimages_to_facts_storage<L: Leaf + DBObject>(
    preimages: &PreimageMap,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> DbHashMap {
    let mut storage = DbHashMap::new();
    for (hash, preimage) in preimages {
        let db_object = fact_db_node_from_preimage::<L>(*hash, preimage);
        let key = db_object.get_db_key(key_context, &db_object.0.hash.0.to_bytes_be());
        storage
            .insert(key, db_object.serialize().expect("facts node serialization should succeed"));
    }
    storage
}

fn add_class_leaves(
    storage: &mut DbHashMap,
    class_leaves: &HashMap<ClassHash, CommitterCompiledClassHash>,
) {
    let key_context = EmptyKeyContext;
    for compiled_class_hash in class_leaves.values() {
        insert_filled_node(
            storage,
            &key_context,
            FilledNode {
                hash: TreeHashFunctionImpl::compute_leaf_hash(compiled_class_hash),
                data: NodeData::Leaf(*compiled_class_hash),
            },
        );
    }
}

fn add_contract_leaves(
    storage: &mut DbHashMap,
    contract_leaves: &HashMap<ContractAddress, ContractState>,
) {
    let key_context = EmptyKeyContext;
    for contract_state in contract_leaves.values() {
        insert_filled_node(
            storage,
            &key_context,
            FilledNode {
                hash: TreeHashFunctionImpl::compute_leaf_hash(contract_state),
                data: NodeData::Leaf(contract_state.clone()),
            },
        );
    }
}

fn add_storage_leaves(
    storage: &mut DbHashMap,
    contract_address: ContractAddress,
    storage_leaves: &HashMap<StarknetStorageKey, StarknetStorageValue>,
) {
    for storage_value in storage_leaves.values() {
        insert_filled_node(
            storage,
            &contract_address,
            FilledNode {
                hash: TreeHashFunctionImpl::compute_leaf_hash(storage_value),
                data: NodeData::Leaf(*storage_value),
            },
        );
    }
}

fn classes_witness_to_facts_storage(
    preimages: &PreimageMap,
    class_leaves: &HashMap<ClassHash, CommitterCompiledClassHash>,
) -> DbHashMap {
    let mut storage =
        preimages_to_facts_storage::<CommitterCompiledClassHash>(preimages, &EmptyKeyContext);
    add_class_leaves(&mut storage, class_leaves);
    storage
}

fn contracts_witness_to_facts_storage(
    preimages: &PreimageMap,
    contract_leaves: &HashMap<ContractAddress, ContractState>,
) -> DbHashMap {
    let mut storage = preimages_to_facts_storage::<ContractState>(preimages, &EmptyKeyContext);
    add_contract_leaves(&mut storage, contract_leaves);
    storage
}

fn storage_witness_to_facts_storage(
    preimages: &PreimageMap,
    contract_address: ContractAddress,
    storage_leaves: &HashMap<StarknetStorageKey, StarknetStorageValue>,
) -> DbHashMap {
    let mut storage =
        preimages_to_facts_storage::<StarknetStorageValue>(preimages, &contract_address);
    add_storage_leaves(&mut storage, contract_address, storage_leaves);
    storage
}

/// To verify that the returned PreImageMap indeed contains all the witness paths from the root
/// towards the requested leaves, we re-fetch the paths from a "mock storage" that only consists of
/// the nodes in the returned PreImageMap and the leaves.
async fn verify_preimage_map_paths_exist<L>(
    witness_nodes: &PreimageMap,
    root_hash: HashOutput,
    leaf_indices: &mut [NodeIndex],
    storage_map: DbHashMap,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) where
    L: Leaf + DBObject,
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    let mut storage = MapStorage(storage_map);
    let sorted_leaf_indices = SortedLeafIndices::new(leaf_indices);
    let fetched = fetch_patricia_paths::<L, FactsNodeLayout>(
        &mut storage,
        root_hash,
        sorted_leaf_indices,
        None,
        key_context,
    )
    .await
    .unwrap_or_else(|error| panic!("failed to re-fetch Patricia paths: {error:?}"));

    for (hash, preimage) in witness_nodes {
        assert_eq!(
            fetched.get(hash),
            Some(preimage),
            "witness node {hash:?} is not on a valid Patricia path"
        );
    }
}

async fn verify_witness_patricia_paths(
    patricia_proofs: &StarknetForestProofs,
    accessed_keys: &AccessedKeys,
    class_leaves: &HashMap<ClassHash, CommitterCompiledClassHash>,
    storage_leaves: &HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
    classes_trie_root: HashOutput,
    contracts_trie_root: HashOutput,
) {
    let mut class_leaf_indices: Vec<NodeIndex> =
        accessed_keys.accessed_class_hashes.iter().map(class_hash_into_node_index).collect();
    verify_preimage_map_paths_exist::<CommitterCompiledClassHash>(
        &patricia_proofs.classes_trie_proof,
        classes_trie_root,
        &mut class_leaf_indices,
        classes_witness_to_facts_storage(&patricia_proofs.classes_trie_proof, class_leaves),
        &EmptyKeyContext,
    )
    .await;

    let mut contract_leaf_indices: Vec<NodeIndex> =
        accessed_keys.accessed_contracts.iter().map(contract_address_into_node_index).collect();
    verify_preimage_map_paths_exist::<ContractState>(
        &patricia_proofs.contracts_trie_proof.nodes,
        contracts_trie_root,
        &mut contract_leaf_indices,
        contracts_witness_to_facts_storage(
            &patricia_proofs.contracts_trie_proof.nodes,
            &patricia_proofs.contracts_trie_proof.leaves,
        ),
        &EmptyKeyContext,
    )
    .await;

    let contract_storage_keys = contract_storage_keys(accessed_keys);
    for contract_address in &accessed_keys.accessed_contracts {
        let storage_proof = patricia_proofs
            .contracts_trie_storage_proofs
            .get(contract_address)
            .unwrap_or_else(|| panic!("missing storage trie proof for {contract_address:?}"));
        let contract_state = patricia_proofs
            .contracts_trie_proof
            .leaves
            .get(contract_address)
            .unwrap_or_else(|| panic!("missing contracts trie leaf for {contract_address:?}"));
        let storage_keys = contract_storage_keys
            .get(contract_address)
            .unwrap_or_else(|| panic!("missing accessed storage keys for {contract_address:?}"));
        let mut storage_leaf_indices: Vec<NodeIndex> =
            storage_keys.iter().map(NodeIndex::from).collect();
        verify_preimage_map_paths_exist::<StarknetStorageValue>(
            storage_proof,
            contract_state.storage_root_hash,
            &mut storage_leaf_indices,
            storage_witness_to_facts_storage(
                storage_proof,
                *contract_address,
                storage_leaves.get(contract_address).unwrap_or_else(|| {
                    panic!("missing storage leaves for contract {contract_address:?}")
                }),
            ),
            contract_address,
        )
        .await;
    }
}

async fn assert_witnesses_and_digest_present(
    committer: &mut ApolloTestCommitter,
    height: BlockNumber,
    accessed_keys: &AccessedKeys,
    expected_patricia_proofs: &StarknetForestProofs,
) {
    assert_eq!(
        committer.load_witnesses_digest(height).await.unwrap(),
        Some(expected_accessed_keys_digest(accessed_keys)),
    );
    assert_eq!(
        committer.forest_storage.read_witnesses(height).await.unwrap().as_ref(),
        Some(expected_patricia_proofs),
    );
}

async fn assert_witnesses_and_digest_absent(
    committer: &mut ApolloTestCommitter,
    height: BlockNumber,
) {
    assert!(committer.load_witnesses_digest(height).await.unwrap().is_none());
    assert!(committer.forest_storage.read_witnesses(height).await.unwrap().is_none());
}
