use std::marker::PhantomData;

use crate::felt::Felt;
use crate::hash::hash_trait::{HashFunction, HashInputPair, HashOutput};
use crate::hash::poseidon::PoseidonHashFunction;
use crate::patricia_merkle_tree::node_data::inner_node::{
    BinaryData, EdgeData, NodeData, PathToBottom,
};
use crate::patricia_merkle_tree::node_data::leaf::{LeafData, LeafDataImpl};

#[cfg(test)]
#[path = "hash_function_test.rs"]
pub mod hash_function_test;

pub(crate) trait TreeHashFunction<L: LeafData, H: HashFunction> {
    /// Computes the hash of given node data.
    fn compute_node_hash(node_data: &NodeData<L>) -> HashOutput;
}

pub(crate) struct TreeHashFunctionImpl<H: HashFunction> {
    _hash_function: PhantomData<H>,
}

/// Implementation of TreeHashFunction. The implementation is based on the following reference:
/// https://docs.starknet.io/documentation/architecture_and_concepts/Network_Architecture/starknet-state/#trie_construction
// TODO(Aner, 11/4/24): Verify the correctness of the implementation.
pub const CONTRACT_STATE_HASH_VERSION: Felt = Felt::ZERO;
// The hex string corresponding to b'CONTRACT_CLASS_LEAF_V0' in big-endian.
pub const CONTRACT_CLASS_LEAF_V0: &str = "0x434f4e54524143545f434c4153535f4c4541465f5630";
impl<H: HashFunction> TreeHashFunction<LeafDataImpl, H> for TreeHashFunctionImpl<H> {
    fn compute_node_hash(node_data: &NodeData<LeafDataImpl>) -> HashOutput {
        match node_data {
            NodeData::Binary(BinaryData {
                left_hash,
                right_hash,
            }) => H::compute_hash(HashInputPair(left_hash.0, right_hash.0)),
            NodeData::Edge(EdgeData {
                bottom_hash: hash_output,
                path_to_bottom: PathToBottom { path, length },
            }) => HashOutput(
                H::compute_hash(HashInputPair(hash_output.0, path.into())).0 + Felt::from(length.0),
            ),
            NodeData::Leaf(LeafDataImpl::StorageValue(storage_value)) => HashOutput(*storage_value),
            NodeData::Leaf(LeafDataImpl::CompiledClassHash(compiled_class_hash)) => {
                let contract_class_leaf_version: Felt = Felt::from_hex(CONTRACT_CLASS_LEAF_V0)
                    .expect(
                    "could not parse hex string corresponding to b'CONTRACT_CLASS_LEAF_V0' to Felt",
                );
                // TODO(Aner, 19/05/2024): remove\modify generics in TreeHashFunctionImpl.
                PoseidonHashFunction::compute_hash(HashInputPair(
                    contract_class_leaf_version,
                    compiled_class_hash.0,
                ))
            }
            NodeData::Leaf(LeafDataImpl::ContractState(contract_state)) => {
                H::compute_hash(HashInputPair(
                    H::compute_hash(HashInputPair(
                        H::compute_hash(HashInputPair(
                            contract_state.class_hash.0,
                            contract_state.storage_root_hash.0,
                        ))
                        .0,
                        contract_state.nonce.0,
                    ))
                    .0,
                    CONTRACT_STATE_HASH_VERSION,
                ))
            }
        }
    }
}
