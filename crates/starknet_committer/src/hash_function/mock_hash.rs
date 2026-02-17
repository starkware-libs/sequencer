use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::external_test_utils::AdditionHash;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::NodeData;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_types_core::hash::StarkHash;

use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
};
use crate::hash_function::hash::{PedersenHashFunction, PoseidonHashFunction};

pub struct MockTreeHashFunction;

impl TreeHashFunction<IndexLayoutContractState> for MockTreeHashFunction {
    fn compute_leaf_hash(contract_state: &IndexLayoutContractState) -> HashOutput {
        HashOutput(AdditionHash::hash_array(
            vec![
                contract_state.0.class_hash.0,
                contract_state.0.storage_root_hash.0,
                contract_state.0.nonce.0,
            ]
            .as_slice(),
        ))
    }

    fn compute_node_hash(node_data: &NodeData<IndexLayoutContractState, HashOutput>) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<PedersenHashFunction>(node_data)
    }
}

impl TreeHashFunction<IndexLayoutCompiledClassHash> for MockTreeHashFunction {
    fn compute_leaf_hash(compiled_class_hash: &IndexLayoutCompiledClassHash) -> HashOutput {
        HashOutput(compiled_class_hash.0.0)
    }
    fn compute_node_hash(
        node_data: &NodeData<IndexLayoutCompiledClassHash, HashOutput>,
    ) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<PoseidonHashFunction>(node_data)
    }
}

impl TreeHashFunction<IndexLayoutStarknetStorageValue> for MockTreeHashFunction {
    fn compute_leaf_hash(leaf_data: &IndexLayoutStarknetStorageValue) -> HashOutput {
        HashOutput(leaf_data.0.0)
    }
    fn compute_node_hash(
        node_data: &NodeData<IndexLayoutStarknetStorageValue, HashOutput>,
    ) -> HashOutput {
        Self::compute_node_hash_with_inner_hash_function::<PedersenHashFunction>(node_data)
    }
}
