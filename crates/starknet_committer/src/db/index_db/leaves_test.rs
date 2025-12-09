use rstest::rstest;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::db_object::EmptyDeserializationContext;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

#[rstest]
#[case::index_layout_contract_state(IndexLayoutContractState(ContractState {
        class_hash: ClassHash(Felt::from(1)),
        storage_root_hash: HashOutput(Felt::from(2)),
        nonce: Nonce(Felt::from(3)),}
))]
#[case::index_layout_compiled_class_hash(IndexLayoutCompiledClassHash(CompiledClassHash(
    Felt::from(1)
)))]
#[case::index_layout_starknet_storage_value(IndexLayoutStarknetStorageValue(
    StarknetStorageValue(Felt::from(1))
))]
fn test_index_layout_leaf_serde<L: Leaf>(#[case] leaf: L) {
    let serialized = leaf.serialize().unwrap();
    let deserialized = L::deserialize(&serialized, &EmptyDeserializationContext).unwrap();
    assert_eq!(leaf, deserialized);
}
