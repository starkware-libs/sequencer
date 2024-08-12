use starknet_patricia::felt::Felt;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::{
    HashFunction,
    TreeHashFunction,
};
use starknet_types_core::hash::{Pedersen, Poseidon, StarkHash};

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

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
