use starknet_types_core::hash::{Pedersen, Poseidon, StarkHash};

use crate::block_committer::input::StarknetStorageValue;
use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::CompiledClassHash;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    NodeData,
    PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::{ContractState, Leaf};

#[cfg(test)]
#[path = "hash_function_test.rs"]
pub mod hash_function_test;

/// Trait for hash functions.
pub trait HashFunction {
    /// Computes the hash of the given input.
    fn hash(left: &Felt, right: &Felt) -> HashOutput;
}

/// Implementation of HashFunction for Pedersen hash function.
pub struct PedersenHashFunction;
impl HashFunction for PedersenHashFunction {
    fn hash(left: &Felt, right: &Felt) -> HashOutput {
        HashOutput(Felt(Pedersen::hash(&left.0, &right.0)))
    }
}

/// Implementation of HashFunction for Poseidon hash function.
pub struct PoseidonHashFunction;
impl HashFunction for PoseidonHashFunction {
    fn hash(left: &Felt, right: &Felt) -> HashOutput {
        HashOutput(Felt(Poseidon::hash(&left.0, &right.0)))
    }
}

pub trait TreeHashFunction<L: Leaf> {
    /// Computes the hash of the given leaf.
    fn compute_leaf_hash(leaf_data: &L) -> HashOutput;

    /// Computes the hash for the given node data.
    fn compute_node_hash(node_data: &NodeData<L>) -> HashOutput;

    /// The default implementation for internal nodes is based on the following reference:
    /// <https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction>
    fn compute_node_hash_with_inner_hash_function<H: HashFunction>(
        node_data: &NodeData<L>,
    ) -> HashOutput {
        match node_data {
            NodeData::Binary(BinaryData { left_hash, right_hash }) => {
                H::hash(&left_hash.0, &right_hash.0)
            }
            NodeData::Edge(EdgeData {
                bottom_hash: hash_output,
                path_to_bottom: PathToBottom { path, length, .. },
            }) => HashOutput(H::hash(&hash_output.0, &Felt::from(path)).0 + Felt::from(*length)),
            NodeData::Leaf(leaf_data) => Self::compute_leaf_hash(leaf_data),
        }
    }
}

pub struct TreeHashFunctionImpl;

impl TreeHashFunctionImpl {
    // TODO(Aner, 11/4/24): Verify the correctness of the implementation.
    pub const CONTRACT_STATE_HASH_VERSION: Felt = Felt::ZERO;

    // The hex string corresponding to b'CONTRACT_CLASS_LEAF_V0' in big-endian.
    pub const CONTRACT_CLASS_LEAF_V0: &'static str =
        "0x434f4e54524143545f434c4153535f4c4541465f5630";
}

/// Implementation of TreeHashFunction for contracts trie.
/// The implementation is based on the following reference:
/// <https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction>
impl TreeHashFunction<ContractState> for TreeHashFunctionImpl {
    fn compute_leaf_hash(contract_state: &ContractState) -> HashOutput {
        HashOutput(
            Pedersen::hash(
                &Pedersen::hash(
                    &Pedersen::hash(
                        &contract_state.class_hash.0.into(),
                        &contract_state.storage_root_hash.0.into(),
                    ),
                    &contract_state.nonce.0.into(),
                ),
                &Self::CONTRACT_STATE_HASH_VERSION.into(),
            )
            .into(),
        )
    }
    fn compute_node_hash(node_data: &NodeData<ContractState>) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<PedersenHashFunction>(node_data)
    }
}

/// Implementation of TreeHashFunction for the classes trie.
/// The implementation is based on the following reference:
/// <https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction>
impl TreeHashFunction<CompiledClassHash> for TreeHashFunctionImpl {
    fn compute_leaf_hash(compiled_class_hash: &CompiledClassHash) -> HashOutput {
        let contract_class_leaf_version: Felt = Felt::from_hex(Self::CONTRACT_CLASS_LEAF_V0)
            .expect(
                "could not parse hex string corresponding to b'CONTRACT_CLASS_LEAF_V0' to Felt",
            );
        HashOutput(
            Poseidon::hash(&contract_class_leaf_version.into(), &compiled_class_hash.0.into())
                .into(),
        )
    }
    fn compute_node_hash(node_data: &NodeData<CompiledClassHash>) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<PoseidonHashFunction>(node_data)
    }
}

/// Implementation of TreeHashFunction for the storage trie.
/// The implementation is based on the following reference:
/// <https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction>
impl TreeHashFunction<StarknetStorageValue> for TreeHashFunctionImpl {
    fn compute_leaf_hash(storage_value: &StarknetStorageValue) -> HashOutput {
        HashOutput(storage_value.0)
    }
    fn compute_node_hash(node_data: &NodeData<StarknetStorageValue>) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<PedersenHashFunction>(node_data)
    }
}

/// Combined trait for all specific implementations.
pub(crate) trait ForestHashFunction:
    TreeHashFunction<ContractState>
    + TreeHashFunction<CompiledClassHash>
    + TreeHashFunction<StarknetStorageValue>
{
}
impl<T> ForestHashFunction for T where
    T: TreeHashFunction<ContractState>
        + TreeHashFunction<CompiledClassHash>
        + TreeHashFunction<StarknetStorageValue>
{
}
