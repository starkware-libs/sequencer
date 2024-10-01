use std::fmt::Debug;

use rstest::rstest;
use starknet_patricia::felt::Felt;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::storage::db_object::Deserializable;
use starknet_patricia::storage::storage_trait::StorageValue;

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

#[rstest]
fn test_deserialize_contract_state_without_nonce() {
    // Simulate a serialized JSON without the "nonce" field.
    let serialized = StorageValue(
        r#"
        {
            "contract_hash": "0x1234abcd",
            "storage_commitment_tree": {
                "root": "0x5678"
            }
        }
        "#
        .as_bytes()
        .to_vec(),
    );

    let contract_state = ContractState::deserialize(&serialized).unwrap();

    // Validate the fields (nonce should be the default "0")
    assert_eq!(contract_state.nonce, Nonce::from_hex("0x0").unwrap());
    assert_eq!(contract_state.class_hash, ClassHash::from_hex("0x1234abcd").unwrap());
    assert_eq!(contract_state.storage_root_hash, HashOutput::from_hex("0x5678").unwrap());
}
