use blockifier::state::cached_state::StateMaps;
use indexmap::IndexMap;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FactDbFilledNode;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaPrefix;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{create_db_key, DbKeyPrefix};
use starknet_rust_core::types::{Felt, MerkleNode, StorageProof as RpcStorageProof};

use crate::errors::ProofProviderError;
use crate::storage_proofs::RpcStorageProofsQuery;

/// Builds a FactsDb from RPC storage proofs and execution initial reads.
///
/// This stores:
/// - Inner nodes for contracts, classes, and storage proofs.
/// - Contract state leaves from the RPC proof.
/// - Storage leaves and compiled class leaves from initial reads.
#[allow(dead_code)]
pub(crate) fn create_facts_db_from_storage_proof(
    rpc_proof: &RpcStorageProof,
    query: &RpcStorageProofsQuery,
    initial_reads: &StateMaps,
) -> Result<FactsDb<MapStorage>, ProofProviderError> {
    let mut storage = MapStorage::default();

    insert_inner_nodes(&mut storage, &rpc_proof.contracts_proof.nodes)?;
    insert_inner_nodes(&mut storage, &rpc_proof.classes_proof)?;
    for storage_proof in &rpc_proof.contracts_storage_proofs {
        insert_inner_nodes(&mut storage, storage_proof)?;
    }

    insert_contract_leaves(&mut storage, rpc_proof, query)?;
    insert_storage_leaves(&mut storage, initial_reads)?;
    insert_compiled_class_leaves(&mut storage, initial_reads)?;

    Ok(FactsDb::new(storage))
}

/// Inserts binary/edge nodes for a proof into facts storage.
fn insert_inner_nodes<S: std::hash::BuildHasher>(
    storage: &mut MapStorage,
    nodes: &IndexMap<Felt, MerkleNode, S>,
) -> Result<(), ProofProviderError> {
    for (hash, node) in nodes {
        let node_data: NodeData<StarknetStorageValue, HashOutput> = NodeData::from(node);
        // The leaf type is irrelevant here because we only serialize Binary/Edge nodes.
        let filled_node =
            FactDbFilledNode::<StarknetStorageValue> { hash: HashOutput(*hash), data: node_data };
        let value = filled_node.serialize()?;
        let node_prefix: DbKeyPrefix = PatriciaPrefix::InnerNode.into();
        let key = create_db_key(node_prefix, &hash.to_bytes_be());
        storage.0.insert(key, value);
    }

    Ok(())
}

/// Helper to insert a leaf node into facts storage.
///
/// Creates a `FactDbFilledNode` from the leaf, computes the key using the leaf's context,
/// serializes it, and inserts it into storage.
fn insert_leaf_node<L: Leaf>(
    storage: &mut MapStorage,
    leaf_hash: HashOutput,
    leaf: L,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) -> Result<(), ProofProviderError> {
    let filled_node = FactDbFilledNode { hash: leaf_hash, data: NodeData::Leaf(leaf) };
    let key = filled_node.db_key(key_context);
    let serialized = filled_node.serialize()?;
    storage.0.insert(key, serialized);
    Ok(())
}

/// Inserts contract state leaves (class hash, nonce, storage root) from the RPC proof.
fn insert_contract_leaves(
    storage: &mut MapStorage,
    rpc_proof: &RpcStorageProof,
    query: &RpcStorageProofsQuery,
) -> Result<(), ProofProviderError> {
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
        insert_leaf_node(storage, leaf_hash, contract_state, &EmptyKeyContext)?;
    }

    Ok(())
}

/// Inserts storage leaves derived from execution initial reads.
fn insert_storage_leaves(
    storage: &mut MapStorage,
    initial_reads: &StateMaps,
) -> Result<(), ProofProviderError> {
    for ((address, _key), value) in &initial_reads.storage {
        let storage_value = StarknetStorageValue(*value);
        let leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&storage_value);
        insert_leaf_node(storage, leaf_hash, storage_value, address)?;
    }

    Ok(())
}

/// Inserts compiled class hash leaves from execution initial reads.
fn insert_compiled_class_leaves(
    storage: &mut MapStorage,
    initial_reads: &StateMaps,
) -> Result<(), ProofProviderError> {
    for class_hash_value in initial_reads.compiled_class_hashes.values() {
        // TODO(Aviv): Delete Compiled class hash type from the committer and use the type from the
        // api crate.
        let compiled_class_hash = CompiledClassHash(class_hash_value.0);
        let leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&compiled_class_hash);
        insert_leaf_node(storage, leaf_hash, compiled_class_hash, &EmptyKeyContext)?;
    }

    Ok(())
}
