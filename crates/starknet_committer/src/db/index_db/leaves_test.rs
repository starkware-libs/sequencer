use rstest::rstest;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia_storage::db_object::EmptyDeserializationContext;
use starknet_patricia_storage::storage_trait::DbValue;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::StarknetStorageValue;
use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

fn contract_state_leaf() -> IndexLayoutContractState {
    IndexLayoutContractState(ContractState {
        class_hash: ClassHash(Felt::from(1)),
        storage_root_hash: HashOutput(Felt::from(2)),
        nonce: Nonce(Felt::from(3)),
    })
}

fn compiled_class_hash_leaf() -> IndexLayoutCompiledClassHash {
    IndexLayoutCompiledClassHash(CompiledClassHash(Felt::ONE))
}

fn starknet_storage_value_leaf() -> IndexLayoutStarknetStorageValue {
    IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::ONE))
}

fn starknet_storage_value_leaf_96_bits() -> IndexLayoutStarknetStorageValue {
    // 2^96 (12 bytes, under the 27 nibbles threshold)
    IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::from(1_u128 << 95)))
}

fn starknet_storage_value_leaf_136_bits() -> IndexLayoutStarknetStorageValue {
    // 2^136 (reaching the 34 nibbles / 17 bytes serialization threshold)
    IndexLayoutStarknetStorageValue(StarknetStorageValue(Felt::from_bytes_be(&[
        0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8,
        128_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8,
        0_u8, 0_u8,
    ])))
}

#[rstest]
#[case::index_layout_contract_state(contract_state_leaf())]
#[case::index_layout_compiled_class_hash(compiled_class_hash_leaf())]
#[case::index_layout_starknet_storage_value(starknet_storage_value_leaf())]
fn test_index_layout_leaf_serde<L: Leaf>(#[case] leaf: L) {
    let serialized = leaf.serialize().unwrap();
    let deserialized = L::deserialize(&serialized, &EmptyDeserializationContext).unwrap();
    assert_eq!(leaf, deserialized);
}

#[rstest]
#[case(contract_state_leaf(), DbValue(vec![1, 2, 3]))]
#[case(compiled_class_hash_leaf(), DbValue(vec![1]))]
#[case(starknet_storage_value_leaf(), DbValue(vec![1]))]
// We are serializing 2^96. The 4 MSB of the first byte are the chooser. For values >= 16 but under
// 27 nibbles, the chooser is the number of bytes. In this case, the first byte will be 11000000
// (chooser 12, i.e. we need 12 bytes) followed by the value.
#[case(starknet_storage_value_leaf_96_bits(), DbValue(vec![192, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]))]
// We are serializing 2^136, which exceeds the 34 nibbles threshold where the encoding utilizes the
// full 32 bytes. This case is marked by chooser = 15, followed by the value, starting immediately
// after the chooser (hence the first 116 bits after the chooser are 0).
#[case(starknet_storage_value_leaf_136_bits(), DbValue(vec![
    240, 0, 0, 0, 0, 0, 0, 0, 
    0, 0, 0, 0, 0, 0, 0, 128,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0
]))]
fn test_leaf_serialization_regression<L: Leaf>(
    #[case] leaf: L,
    #[case] expected_serialize: DbValue,
) {
    let actual_serialize = leaf.serialize().unwrap();
    assert_eq!(actual_serialize, expected_serialize);
}
