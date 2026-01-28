//! Functions for creating FactsDb from storage proofs and committing state diffs.
//!
//! This module provides utilities to:
//! - Populate a FactsDb with Patricia trie nodes extracted from RPC storage proofs
//! - Convert execution outputs to state diffs
//! - Commit state diffs to compute new state roots
//!
//! The implementation uses `FilledNode` and the `DBObject` trait for serialization,
//! ensuring consistency with the rest of the codebase.

use std::collections::HashSet;
use std::hash::BuildHasher;

use blockifier::blockifier::transaction_executor::TransactionExecutionOutput;
use blockifier::state::cached_state::{StateMaps, StorageDiff, StorageView};
use ethnum::U256;
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
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::db::facts_db::types::FactsDbInitialRead;
use starknet_committer::db::forest_trait::ForestWriter;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    NodeData,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{create_db_key, DbKeyPrefix, DbValue};
use starknet_rust_core::types::{
    ContractLeafData,
    Felt,
    MerkleNode,
    StorageProof as RpcStorageProof,
};

/// Computes missing parent nodes for modified leaves.
///
/// RPC proofs contain nodes along paths to leaves, sufficient for verification (bottom-up).
/// However, the committer needs node data for traversal (top-down). For modified leaves,
/// we reconstruct parent nodes from the proof data.
///
/// This function:
/// 1. Identifies modified contract addresses, storage keys, and class hashes from the state diff
/// 2. For each modified leaf, examines the proof nodes
/// 3. Reconstructs parent nodes from child + sibling information when needed
///
/// The key insight: when we have both children of a binary node (or an edge's child),
/// we can reconstruct the parent node's data structure.
fn compute_missing_nodes_for_modified_leaves(
    _rpc_proof: &RpcStorageProof,
    _state_diff: &StateDiff,
) -> MapStorage {
    // TODO: Implement node reconstruction logic
    // For now, return empty storage as a placeholder
    // The actual implementation will:
    // 1. Trace paths in the proof for each modified address/key
    // 2. Identify missing parent nodes
    // 3. Reconstruct them from available data
    MapStorage::default()
}

/// Creates a FactsDb directly from RpcStorageProof.
///
/// This function converts `MerkleNode`s to `FilledNode`s and uses the `DBObject` trait
/// for serialization, ensuring consistency with the rest of the codebase.
///
/// The function processes:
/// - Inner nodes from all proofs (classes, contracts, storage) with `patricia_node` prefix
/// - Leaves with appropriate prefixes:
///   - Contract leaves from `contract_leaves_data` → `contract_state` prefix
///   - Class leaves (from edge.child where length == 1) → `contract_class_leaf` prefix
///   - Storage leaves (from edge.child where length == 1) → `starknet_storage_leaf` prefix
///
/// For edge nodes with `length == 1`, the `child` field contains the leaf value directly
/// (for classes and storage tries).
///
/// # Arguments
///
/// * `rpc_proof` - The RPC storage proof containing merkle nodes and leaves
/// * `state_diff` - The state diff to determine which leaves were modified (used for computing
///   missing nodes)
pub fn create_facts_db_from_storage_proof(
    rpc_proof: &RpcStorageProof,
    state_diff: &StateDiff,
) -> FactsDb<MapStorage> {
    let mut storage = MapStorage::default();
    let key_context = EmptyKeyContext;

    // Process classes proof - inner nodes + leaves for edges with length == 1
    // For classes, edge.child IS the leaf value (CompiledClassHash)
    add_nodes_with_simple_leaves(&mut storage, &rpc_proof.classes_proof, CompiledClassHash);

    // Process contracts proof - inner nodes only (leaves come from contract_leaves_data)
    add_filled_nodes::<ContractState>(&mut storage, &rpc_proof.contracts_proof.nodes, &key_context);

    // Add contract leaves directly from contract_leaves_data
    for leaf_data in &rpc_proof.contracts_proof.contract_leaves_data {
        let filled_node = contract_leaf_to_filled_node(leaf_data);
        let db_key = filled_node.db_key(&key_context);
        let db_value = filled_node.serialize().expect("ContractState serialization should succeed");
        storage.0.insert(db_key, db_value);
    }

    // Process storage proofs - add all inner nodes
    // Storage proofs contain nodes that are referenced by contract leaves (via storage_root)
    // We add the inner nodes (Binary/Edge) without trying to create leaf nodes
    // Note: We use ContractState as the leaf type parameter because inner nodes always use
    // the patricia_node prefix regardless of actual leaf type, and ContractState uses
    // EmptyKeyContext
    for storage_proof in &rpc_proof.contracts_storage_proofs {
        for (hash, node) in storage_proof {
            // Add inner nodes only - these use patricia_node prefix regardless of leaf type
            let filled_node: FilledNode<ContractState, HashOutput> =
                merkle_node_to_filled_node(*hash, node);
            let db_key = filled_node.db_key(&key_context);
            let db_value =
                filled_node.serialize().expect("Storage node serialization should succeed");
            storage.0.insert(db_key, db_value);
        }
    }

    // Add dummy edges for orphan child hashes (sibling hashes without preimages)
    // add_orphan_child_hashes(&mut storage, &rpc_proof.contracts_proof.nodes);
    // for storage_proof in &rpc_proof.contracts_storage_proofs {
    //     add_orphan_child_hashes(&mut storage, storage_proof);
    // }

    // Compute and add missing nodes for modified leaves
    let computed_nodes = compute_missing_nodes_for_modified_leaves(rpc_proof, state_diff);
    storage.0.extend(computed_nodes.0);

    FactsDb::new(storage)
}

/// Converts a MerkleNode to a FilledNode with inner node data (Binary or Edge).
///
/// The leaf type parameter `L` is used for type consistency but doesn't affect
/// inner node serialization or prefix (all inner nodes use `patricia_node` prefix).
fn merkle_node_to_filled_node<L: Leaf>(hash: Felt, node: &MerkleNode) -> FilledNode<L, HashOutput> {
    let data = match node {
        MerkleNode::BinaryNode(bn) => NodeData::Binary(BinaryData {
            left_data: HashOutput(bn.left),
            right_data: HashOutput(bn.right),
        }),
        MerkleNode::EdgeNode(en) => {
            let path = EdgePath(U256::from_be_bytes(en.path.to_bytes_be()));
            let length =
                EdgePathLength::new(en.length as u8).expect("Edge length should fit in u8");
            let path_to_bottom =
                PathToBottom::new(path, length).expect("PathToBottom creation should succeed");

            NodeData::Edge(EdgeData { bottom_data: HashOutput(en.child), path_to_bottom })
        }
    };

    FilledNode { hash: HashOutput(hash), data }
}

/// Converts ContractLeafData to a FilledNode with leaf data.
///
/// Computes the leaf hash using TreeHashFunctionImpl and wraps the ContractState
/// in a FilledNode for consistent serialization via DBObject.
fn contract_leaf_to_filled_node(
    leaf_data: &ContractLeafData,
) -> FilledNode<ContractState, HashOutput> {
    let contract_state = ContractState {
        class_hash: ClassHash(leaf_data.class_hash),
        nonce: Nonce(leaf_data.nonce),
        storage_root_hash: HashOutput(leaf_data.storage_root.unwrap_or(Felt::ZERO)),
    };

    let hash = TreeHashFunctionImpl::compute_leaf_hash(&contract_state);
    println!(" The hash of the contract state {:?} is {:?}", contract_state, hash);

    FilledNode { hash, data: NodeData::Leaf(contract_state) }
}

/// Adds MerkleNodes to storage by converting them to FilledNodes and using DBObject.
///
/// All MerkleNodes in RPC proofs represent inner nodes (binary/edge).
/// The serialization and prefix determination is handled by the DBObject trait impl.
fn add_filled_nodes<L: Leaf>(
    storage: &mut MapStorage,
    nodes: &IndexMap<Felt, MerkleNode, impl BuildHasher>,
    key_context: &<L as HasStaticPrefix>::KeyContext,
) {
    for (hash, node) in nodes {
        let filled_node: FilledNode<L, HashOutput> = merkle_node_to_filled_node(*hash, node);
        let db_key = filled_node.db_key(key_context);
        let db_value = filled_node.serialize().expect("Node serialization should succeed");
        storage.0.insert(db_key, db_value);
    }
}

/// Adds MerkleNodes to storage, including leaves for edges with length == 1.
///
/// For simple Felt-based leaves (CompiledClassHash, StarknetStorageValue), when an edge
/// has length == 1, the `child` field IS the leaf value directly.
///
/// The `leaf_from_felt` closure constructs the leaf type from the edge's child value.
fn add_nodes_with_simple_leaves<L>(
    storage: &mut MapStorage,
    nodes: &IndexMap<Felt, MerkleNode, impl BuildHasher>,
    leaf_from_felt: impl Fn(Felt) -> L,
) where
    L: Leaf + HasStaticPrefix<KeyContext = EmptyKeyContext>,
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    let key_context = EmptyKeyContext;

    for (hash, node) in nodes {
        // Store the inner node (binary or edge)
        let filled_node: FilledNode<L, HashOutput> = merkle_node_to_filled_node(*hash, node);
        let db_key = filled_node.db_key(&key_context);
        let db_value = filled_node.serialize().expect("Node serialization should succeed");
        storage.0.insert(db_key, db_value);

        // For edge nodes with length == 1, also store the leaf
        if let MerkleNode::EdgeNode(edge) = node {
            if edge.length == 1 {
                let leaf_value = leaf_from_felt(edge.child);
                let leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&leaf_value);
                let leaf_node = FilledNode { hash: leaf_hash, data: NodeData::Leaf(leaf_value) };
                let leaf_db_key = leaf_node.db_key(&key_context);
                let leaf_db_value =
                    leaf_node.serialize().expect("Leaf serialization should succeed");
                storage.0.insert(leaf_db_key, leaf_db_value);
            }
        }
    }
}

/// Adds dummy edge nodes for orphan child hashes that don't have preimages.
///
/// RPC storage proofs provide sibling hashes for verification but don't include their preimages.
/// The committer needs to read these nodes, so we create dummy edge nodes pointing to a dummy leaf.
/// This allows the committer to traverse without errors, as it won't traverse into unmodified
/// subtrees.
///
/// For each child hash referenced in nodes but not present in storage:
/// 1. Create a dummy edge node at `patricia_node:<hash>` pointing to a shared dummy leaf
/// 2. The dummy leaf is a ContractState with all-zero values
fn add_orphan_child_hashes(
    storage: &mut MapStorage,
    nodes: &IndexMap<Felt, MerkleNode, impl BuildHasher>,
) {
    // Build set of all hashes already in storage (extract from keys)
    let mut existing_hashes: HashSet<[u8; 32]> = HashSet::new();
    for key in storage.0.keys() {
        // Keys are formatted as "prefix:hash" where hash is 32 bytes at the end
        if key.0.len() >= 32 {
            let suffix = &key.0[key.0.len() - 32..];
            if let Ok(arr) = <[u8; 32]>::try_from(suffix) {
                existing_hashes.insert(arr);
            }
        }
    }

    // Also add all hashes from nodes (they're stored as patricia_node)
    for (hash, _) in nodes {
        existing_hashes.insert(hash.to_bytes_be());
    }

    // Collect all child hashes referenced by nodes
    let mut child_hashes: HashSet<Felt> = HashSet::new();
    for (_, node) in nodes {
        match node {
            MerkleNode::BinaryNode(bn) => {
                child_hashes.insert(bn.left);
                child_hashes.insert(bn.right);
            }
            MerkleNode::EdgeNode(en) => {
                child_hashes.insert(en.child);
            }
        }
    }

    // Create dummy leaf (only once)
    let dummy_leaf = ContractState {
        class_hash: ClassHash(Felt::ZERO),
        nonce: Nonce(Felt::ZERO),
        storage_root_hash: HashOutput(Felt::ZERO),
    };
    let dummy_leaf_hash = TreeHashFunctionImpl::compute_leaf_hash(&dummy_leaf);

    let leaf_prefix = DbKeyPrefix::new(b"contract_state".into());
    let leaf_key = create_db_key(leaf_prefix, &dummy_leaf_hash.0.to_bytes_be());
    let leaf_value = dummy_leaf.serialize().expect("Leaf serialization should succeed");
    storage.0.insert(leaf_key, leaf_value);

    // Binary node value with both children pointing to dummy leaf: [left_hash (32)] [right_hash
    // (32)]
    const SERIALIZE_HASH_BYTES: usize = 32;
    let mut binary_bytes = Vec::with_capacity(SERIALIZE_HASH_BYTES * 2);
    binary_bytes.extend_from_slice(&dummy_leaf_hash.0.to_bytes_be()); // left = dummy leaf
    binary_bytes.extend_from_slice(&dummy_leaf_hash.0.to_bytes_be()); // right = dummy leaf
    let binary_value = DbValue(binary_bytes);

    // Add dummy binary for any child hash not already in storage
    for child_hash in child_hashes {
        let hash_bytes = child_hash.to_bytes_be();
        if !existing_hashes.contains(&hash_bytes) {
            let node_prefix = DbKeyPrefix::new(b"patricia_node".into());
            let db_key = create_db_key(node_prefix, &hash_bytes);
            storage.0.insert(db_key, binary_value.clone());
        }
    }
}

// =============================================================================
// State Diff and Committer Functions
// =============================================================================

/// Creates a StateDiff from transaction execution outputs.
///
/// Combines the StateMaps from all transaction outputs into a single StateDiff
/// that can be passed to the committer.
pub fn create_state_diff_from_execution_outputs(
    execution_outputs: &[TransactionExecutionOutput],
) -> StateDiff {
    // Combine all StateMaps from execution outputs
    let mut combined = StateMaps::default();
    for (_, state_maps) in execution_outputs {
        combined.extend(state_maps);
    }

    // Convert to committer StateDiff
    create_committer_state_diff(combined)
}

/// Creates a committer StateDiff from blockifier StateMaps.
///
/// Converts the blockifier's state representation to the committer's format.
fn create_committer_state_diff(state_maps: StateMaps) -> StateDiff {
    StateDiff {
        address_to_class_hash: state_maps.class_hashes,
        address_to_nonce: state_maps.nonces,
        class_hash_to_compiled_class_hash: state_maps
            .compiled_class_hashes
            .into_iter()
            .map(|(k, v)| (k, CompiledClassHash(v.0)))
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

/// Commits the state diff and returns the computed new state roots.
///
/// This function:
/// 1. Creates the committer input with the state diff and previous roots
/// 2. Runs the commit_block to compute new state roots
/// 3. Writes the new commitments back to the FactsDb
///
/// # Arguments
///
/// * `facts_db` - The FactsDb containing Patricia trie nodes from storage proofs
/// * `contracts_trie_root_hash` - The previous contracts trie root
/// * `classes_trie_root_hash` - The previous classes trie root
/// * `state_diff` - The state changes to commit
///
/// # Returns
///
/// The new state roots after committing the state diff, or an error.
pub async fn commit_state_diff(
    facts_db: &mut FactsDb<MapStorage>,
    contracts_trie_root_hash: HashOutput,
    classes_trie_root_hash: HashOutput,
    state_diff: StateDiff,
) -> Result<StateRoots, starknet_committer::block_committer::errors::BlockCommitmentError> {
    let config = ReaderConfig::default();
    let initial_read_context =
        FactsDbInitialRead(StateRoots { contracts_trie_root_hash, classes_trie_root_hash });
    let input = Input { state_diff, initial_read_context, config };

    let filled_forest = CommitBlockImpl::commit_block(input, facts_db, None).await?;

    // Write the new commitments to the FactsDb
    facts_db.write(&filled_forest).await;

    Ok(StateRoots {
        contracts_trie_root_hash: filled_forest.get_contract_root_hash(),
        classes_trie_root_hash: filled_forest.get_compiled_class_root_hash(),
    })
}
