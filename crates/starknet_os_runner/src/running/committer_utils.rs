use std::collections::HashSet;
use std::hash::BuildHasher;

use blockifier::state::cached_state::{StateMaps, StorageDiff, StorageView};
use indexmap::IndexMap;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_committer::block_committer::commit::{CommitBlockImpl, CommitBlockTrait};
use starknet_committer::block_committer::input::{
    Input,
    ReaderConfig,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::block_committer::measurements_util::NoMeasurements;
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::db::facts_db::types::FactsDbInitialRead;
use starknet_committer::db::forest_trait::{ForestWriter, StorageInitializer};
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FactDbFilledNode;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::{
    PatriciaPrefix,
    FACT_LAYOUT_DB_KEY_SEPARATOR,
};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{BinaryData, NodeData};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{create_db_key, DbHashMap, DbKeyPrefix, DbValue};
use starknet_rust_core::types::{Felt, MerkleNode, StorageProof as RpcStorageProof};

use crate::errors::ProofProviderError;
use crate::running::storage_proofs::RpcStorageProofsQuery;

/// Converts blockifier's StateMaps to committer's StateDiff format.
pub fn state_maps_to_committer_state_diff(state_maps: StateMaps) -> StateDiff {
    StateDiff {
        address_to_class_hash: state_maps.class_hashes,
        address_to_nonce: state_maps.nonces,
        class_hash_to_compiled_class_hash: state_maps
            .compiled_class_hashes
            .into_iter()
            .map(|(class_hash, compiled_class_hash)| {
                (class_hash, CompiledClassHash(compiled_class_hash.0))
            })
            .collect(),
        storage_updates: StorageDiff::from(StorageView(state_maps.storage))
            .into_iter()
            .map(|(address, updates)| {
                (
                    address,
                    updates
                        .into_iter()
                        .map(|(key, value)| (StarknetStorageKey(key), StarknetStorageValue(value)))
                        .collect(),
                )
            })
            .collect(),
    }
}

/// Validates that the committer state diff contains only allowed state transitions.
///
/// This function enforces the following constraints:
/// * **No Storage Deletions:** Storage entries cannot be updated to `Felt::ZERO`.
/// * **No Class Declarations:** The `class_hash_to_compiled_class_hash` map must be empty.
/// * **No Contract Deployments:** The `address_to_class_hash` map must be empty.
pub(crate) fn validate_virtual_os_state_diff(
    state_diff: &StateDiff,
) -> Result<(), ProofProviderError> {
    // validate no storage deletions.
    for (address, storage_diffs) in &state_diff.storage_updates {
        for (key, value) in storage_diffs {
            if value.0 == Felt::ZERO {
                return Err(ProofProviderError::InvalidStateDiff(format!(
                    "Storage deletion not allowed: try to delete storage at address {address:?}, \
                     key {key:?}"
                )));
            }
        }
    }
    // validate no contract deployments (or replaced classes).
    if !state_diff.address_to_class_hash.is_empty() {
        return Err(ProofProviderError::InvalidStateDiff(format!(
            "Contract deployments not allowed: try to deploy contracts(address to class hash): \
             {0:?}",
            state_diff.address_to_class_hash
        )));
    }
    // validate no contract declarations (or compiled class hash updates).
    if !state_diff.class_hash_to_compiled_class_hash.is_empty() {
        return Err(ProofProviderError::InvalidStateDiff(format!(
            "Contract declarations not allowed: try to declare classes(class hash to compiled \
             class hash): {0:?}",
            state_diff.class_hash_to_compiled_class_hash
        )));
    }
    Ok(())
}

/// Builds a FactsDb from RPC storage proofs and execution initial reads.
///
/// This stores:
/// - Inner nodes for contracts, classes, and storage proofs.
/// - Contract state leaves from the RPC proof.
/// - Storage leaves and compiled class leaves from initial reads.
/// - Dummy binary nodes for orphan child hashes (sibling hashes without preimages).
#[allow(dead_code)]
pub(crate) fn create_facts_db_from_storage_proof(
    rpc_proof: &RpcStorageProof,
    query: &RpcStorageProofsQuery,
    initial_reads: &StateMaps,
) -> Result<FactsDb<MapStorage>, ProofProviderError> {
    let mut db_map: DbHashMap = DbHashMap::new();

    insert_inner_nodes(&mut db_map, &rpc_proof.contracts_proof.nodes)?;
    insert_inner_nodes(&mut db_map, &rpc_proof.classes_proof)?;
    for storage_proof in &rpc_proof.contracts_storage_proofs {
        insert_inner_nodes(&mut db_map, storage_proof)?;
    }

    insert_contract_leaves(&mut db_map, rpc_proof, query)?;
    insert_storage_leaves(&mut db_map, initial_reads)?;
    insert_compiled_class_leaves(&mut db_map, initial_reads)?;

    // Add dummy nodes for orphan hashes (sibling hashes without preimages).
    add_dummy_nodes_for_orphan_hashes(&mut db_map, &rpc_proof.contracts_proof.nodes)?;
    add_dummy_nodes_for_orphan_hashes(&mut db_map, &rpc_proof.classes_proof)?;
    for storage_proof in &rpc_proof.contracts_storage_proofs {
        add_dummy_nodes_for_orphan_hashes(&mut db_map, storage_proof)?;
    }

    Ok(FactsDb::new(MapStorage(db_map)))
}

/// Inserts binary/edge nodes for a proof into facts storage.
fn insert_inner_nodes<S: std::hash::BuildHasher>(
    db_map: &mut DbHashMap,
    nodes: &IndexMap<Felt, MerkleNode, S>,
) -> Result<(), ProofProviderError> {
    for (hash, node) in nodes {
        let node_data: NodeData<StarknetStorageValue, HashOutput> = NodeData::from(node);
        // The leaf type is irrelevant here because we only serialize Binary/Edge nodes.
        let filled_node =
            FactDbFilledNode::<StarknetStorageValue> { hash: HashOutput(*hash), data: node_data };
        let value = filled_node.serialize()?;
        let node_prefix: DbKeyPrefix = PatriciaPrefix::InnerNode.into();
        let key = create_db_key(node_prefix, FACT_LAYOUT_DB_KEY_SEPARATOR, &hash.to_bytes_be());
        db_map.insert(key, value);
    }

    Ok(())
}

/// Helper to insert a leaf node into facts storage.
///
/// Creates a `FactDbFilledNode` from the leaf, computes the key using the leaf's context,
/// serializes it, and inserts it into storage.
fn insert_leaf_node<L: Leaf>(
    db_map: &mut DbHashMap,
    leaf_hash: HashOutput,
    leaf: L,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> Result<(), ProofProviderError> {
    let filled_node = FactDbFilledNode { hash: leaf_hash, data: NodeData::Leaf(leaf) };
    let key = filled_node.get_db_key(key_context, &leaf_hash.0.to_bytes_be());
    let serialized = filled_node.serialize()?;
    db_map.insert(key, serialized);
    Ok(())
}

/// Inserts contract state leaves (class hash, nonce, storage root) from the RPC proof.
fn insert_contract_leaves(
    db_map: &mut DbHashMap,
    rpc_proof: &RpcStorageProof,
    query: &RpcStorageProofsQuery,
) -> Result<(), ProofProviderError> {
    // TODO(Aviv): Consider deleating this validation since it's already validated in the RPC proof.
    let leaves_len = rpc_proof.contracts_proof.contract_leaves_data.len();
    let addresses_len = query.contract_addresses.len();
    if leaves_len != addresses_len {
        return Err(ProofProviderError::InvalidProofResponse(format!(
            "Contract leaves length mismatch: expected {addresses_len} leaves for requested \
             contracts, got {leaves_len}"
        )));
    }

    for (leaf, _address) in
        rpc_proof.contracts_proof.contract_leaves_data.iter().zip(&query.contract_addresses)
    {
        let contract_state = ContractState {
            nonce: Nonce(leaf.nonce),
            class_hash: ClassHash(leaf.class_hash),
            // Empty storage root is represented as 0x0
            storage_root_hash: HashOutput(leaf.storage_root.unwrap_or(Felt::ZERO)),
        };
        let leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&contract_state);
        insert_leaf_node(db_map, leaf_hash, contract_state, &EmptyKeyContext)?;
    }

    Ok(())
}

/// Inserts storage leaves derived from execution initial reads.
fn insert_storage_leaves(
    db_map: &mut DbHashMap,
    initial_reads: &StateMaps,
) -> Result<(), ProofProviderError> {
    for ((address, _storagekey), value) in &initial_reads.storage {
        let storage_value = StarknetStorageValue(*value);
        let leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&storage_value);
        insert_leaf_node(db_map, leaf_hash, storage_value, address)?;
    }

    Ok(())
}

/// Inserts compiled class hash leaves from execution initial reads.
fn insert_compiled_class_leaves(
    db_map: &mut DbHashMap,
    initial_reads: &StateMaps,
) -> Result<(), ProofProviderError> {
    for class_hash_value in initial_reads.compiled_class_hashes.values() {
        let compiled_class_hash = CompiledClassHash(class_hash_value.0);
        let leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&compiled_class_hash);
        insert_leaf_node(db_map, leaf_hash, compiled_class_hash, &EmptyKeyContext)?;
    }

    Ok(())
}

/// Adds a dummy node for an orphan child hash if it doesn't have a preimage.
fn add_dummy_node_for_orphan_child(
    db_map: &mut DbHashMap,
    child_hash: &Felt,
    has_preimage: &HashSet<&Felt>,
    dummy_value: &DbValue,
) {
    if !has_preimage.contains(child_hash) {
        let node_prefix: DbKeyPrefix = PatriciaPrefix::InnerNode.into();
        let key =
            create_db_key(node_prefix, FACT_LAYOUT_DB_KEY_SEPARATOR, &child_hash.to_bytes_be());
        db_map.entry(key).or_insert_with(|| dummy_value.clone());
    }
}

/// Adds dummy binary nodes for orphan child hashes that are referenced but have no preimage.
///
/// RPC storage proofs include sibling hashes for verification but don't provide their preimages.
/// The committer needs to traverse these nodes when deletions are allowed. Since we don't allow
/// deletions, we insert dummy binary nodes (with zero hashes) to satisfy the committer's
/// traversal requirements without requiring full preimages.
fn add_dummy_nodes_for_orphan_hashes(
    db_map: &mut DbHashMap,
    nodes: &IndexMap<Felt, MerkleNode, impl BuildHasher>,
) -> Result<(), ProofProviderError> {
    // Build set of hashes that have preimages in current proof batch.
    let has_preimage: HashSet<&Felt> = nodes.keys().collect();

    // Create dummy binary node value (both children point to zero hash).
    let dummy_hash = HashOutput(Felt::ZERO);
    let dummy_binary = FactDbFilledNode::<StarknetStorageValue> {
        hash: dummy_hash, // Hash field not used for serialization
        data: NodeData::Binary(BinaryData { left_data: dummy_hash, right_data: dummy_hash }),
    };
    let dummy_value = dummy_binary.serialize()?;

    // Insert dummy nodes for orphan child hashes.
    for (_, node) in nodes {
        match node {
            MerkleNode::BinaryNode(bn) => {
                add_dummy_node_for_orphan_child(db_map, &bn.left, &has_preimage, &dummy_value);
                add_dummy_node_for_orphan_child(db_map, &bn.right, &has_preimage, &dummy_value);
            }
            MerkleNode::EdgeNode(en) => {
                add_dummy_node_for_orphan_child(db_map, &en.child, &has_preimage, &dummy_value);
            }
        }
    }

    Ok(())
}

/// Commits the state diff, populates the DB with the new facts and returns the new state roots.
pub async fn commit_state_diff(
    facts_db: &mut FactsDb<MapStorage>,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
    state_diff: StateDiff,
) -> Result<StateRoots, ProofProviderError> {
    let config = ReaderConfig::default();
    let initial_read_context =
        FactsDbInitialRead(StateRoots { contracts_trie_root_hash, classes_trie_root_hash });
    let input = Input { state_diff, initial_read_context, config };

    let filled_forest = CommitBlockImpl::commit_block(input, facts_db, &mut NoMeasurements)
        .await
        .map_err(|e| ProofProviderError::BlockCommitmentError(e.to_string()))?;
    facts_db.write(&filled_forest).await?;

    Ok(StateRoots {
        contracts_trie_root_hash: filled_forest.get_contract_root_hash(),
        classes_trie_root_hash: filled_forest.get_compiled_class_root_hash(),
    })
}
