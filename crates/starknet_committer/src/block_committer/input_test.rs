use rstest::rstest;
use starknet_patricia::felt::Felt;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;

use crate::block_committer::input::ContractAddress;

#[rstest]
fn test_node_index_to_contract_address_conversion() {
    // Positive flow.
    assert_eq!(ContractAddress::try_from(&NodeIndex::FIRST_LEAF), Ok(ContractAddress(Felt::ZERO)));
    assert_eq!(
        ContractAddress::try_from(&(NodeIndex::FIRST_LEAF + NodeIndex(1_u32.into()))),
        Ok(ContractAddress(Felt::ONE))
    );
    assert_eq!(
        ContractAddress::try_from(&NodeIndex::MAX),
        Ok(ContractAddress(Felt::try_from(NodeIndex::MAX - NodeIndex::FIRST_LEAF).unwrap()))
    );

    // Negative flow.
    assert_eq!(
        ContractAddress::try_from(&(NodeIndex::FIRST_LEAF - NodeIndex(1_u32.into()))),
        Err("NodeIndex is not a leaf.".to_string())
    );
}
