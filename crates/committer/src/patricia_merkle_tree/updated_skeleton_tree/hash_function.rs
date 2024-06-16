use starknet_types_core::hash::{Pedersen, Poseidon, StarkHash};

use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData, EdgeData, NodeData, PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::{LeafData, LeafDataImpl};

#[cfg(test)]
#[path = "hash_function_test.rs"]
pub mod hash_function_test;

pub(crate) trait TreeHashFunction<L: LeafData> {
    /// Computes the hash of the given leaf.
    fn compute_leaf_hash(leaf_data: &L) -> HashOutput;

    /// Computes the hash for the given node data.
    /// The default implementation for internal nodes is based on the following reference:
    /// https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction
    fn compute_node_hash(node_data: &NodeData<L>) -> HashOutput {
        match node_data {
            NodeData::Binary(BinaryData {
                left_hash,
                right_hash,
            }) => HashOutput(Pedersen::hash(&left_hash.0.into(), &right_hash.0.into()).into()),
            NodeData::Edge(EdgeData {
                bottom_hash: hash_output,
                path_to_bottom: PathToBottom { path, length, .. },
            }) => HashOutput(
                Felt::from(Pedersen::hash(
                    &hash_output.0.into(),
                    &Felt::from(path).into(),
                )) + Felt::from(*length),
            ),
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

/// Implementation of TreeHashFunction. The implementation is based on the following reference:
/// https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction
impl TreeHashFunction<LeafDataImpl> for TreeHashFunctionImpl {
    fn compute_leaf_hash(leaf_data: &LeafDataImpl) -> HashOutput {
        HashOutput(match leaf_data {
            LeafDataImpl::StorageValue(storage_value) => *storage_value,
            LeafDataImpl::CompiledClassHash(compiled_class_hash) => {
                let contract_class_leaf_version: Felt = Felt::from_hex(
                    Self::CONTRACT_CLASS_LEAF_V0,
                )
                .expect(
                    "could not parse hex string corresponding to b'CONTRACT_CLASS_LEAF_V0' to Felt",
                );
                Poseidon::hash(
                    &contract_class_leaf_version.into(),
                    &compiled_class_hash.0.into(),
                )
                .into()
            }
            LeafDataImpl::ContractState(contract_state) => Pedersen::hash(
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
        })
    }
}
