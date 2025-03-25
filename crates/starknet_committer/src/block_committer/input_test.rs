use starknet_api::core::ContractAddress;
use rstest::rstest;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::try_node_index_into_contract_address;

#[rstest]
fn test_node_index_to_contract_address_conversion() {
    // Positive flow.
    assert_eq!(
        try_node_index_into_contract_address(&NodeIndex::FIRST_LEAF),
        Ok(ContractAddress::try_from(Felt::ZERO).unwrap())
    );
    assert_eq!(
        try_node_index_into_contract_address(&(NodeIndex::FIRST_LEAF + NodeIndex(1_u32.into()))),
        Ok(ContractAddress::try_from(Felt::ONE).unwrap())
    );
    assert_eq!(
        try_node_index_into_contract_address(&NodeIndex::MAX),
        Ok(ContractAddress::try_from(
            Felt::try_from(NodeIndex::MAX - NodeIndex::FIRST_LEAF).unwrap()
        )
        .unwrap())
    );

    // Negative flow.
    assert_eq!(
        try_node_index_into_contract_address(&(NodeIndex::FIRST_LEAF - NodeIndex(1_u32.into()))),
        Err("NodeIndex is not a leaf.".to_string())
    );
}
