use std::fmt::Debug;

use committer::felt::Felt;
use committer::hash::hash_trait::HashOutput;
use committer::patricia_merkle_tree::node_data::leaf::Leaf;
use rstest::rstest;

use crate::block_committer::input::StarknetStorageValue;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{ClassHash, CompiledClassHash, Nonce};

#[rstest]
#[case::zero_storage_leaf(StarknetStorageValue(Felt::ZERO))]
#[case::non_zero_storage_leaf(StarknetStorageValue(Felt::from(999_u128)))]
#[case::zero_compiled_class_leaf(CompiledClassHash(Felt::ZERO))]
#[case::non_zero_compiled_class_leaf(CompiledClassHash(Felt::from(11_u128)))]
#[case::zero_contract_state_leaf(ContractState {
     nonce: Nonce(Felt::ZERO), storage_root_hash: HashOutput(Felt::ZERO), class_hash: ClassHash(Felt::ZERO)
    })
]
#[case::partial_zero_contract_state_leaf(ContractState {
    nonce: Nonce(Felt::ZERO), storage_root_hash: HashOutput(Felt::from(2359743529034_u128)), class_hash: ClassHash(Felt::from(1349866415897798_u128))
   })
]
#[case::without_zero_contract_state_leaf(ContractState {
    nonce: Nonce(Felt::from(23479515749555_u128)), storage_root_hash: HashOutput(Felt::from(2359743529034_u128)), class_hash: ClassHash(Felt::from(1349866415897798_u128))
   })
]
fn test_leaf_serde<L: Leaf + Eq + Debug>(#[case] leaf: L) {
    let serialized = leaf.serialize();
    let deserialized = L::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, leaf);
}

#[rstest]
#[case::storage_leaf(StarknetStorageValue::default())]
#[case::compiled_class_leaf(CompiledClassHash::default())]
#[case::contract_state_leaf(ContractState::default())]
fn test_default_is_empty<L: Leaf>(#[case] leaf: L) {
    assert!(leaf.is_empty())
}
