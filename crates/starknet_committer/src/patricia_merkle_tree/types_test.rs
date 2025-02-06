use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{from_contract_address_for_node_index, StarknetStorageKey};

#[rstest]
fn test_cast_to_node_index(
    #[values(0, 15, 0xDEADBEEF)] leaf_index: u128,
    #[values(true, false)] bool_from_contract_address: bool,
) {
    let expected_node_index = NodeIndex::FIRST_LEAF + leaf_index;
    let actual: NodeIndex = if bool_from_contract_address {
        from_contract_address_for_node_index(
            &ContractAddress::try_from(Felt::from(leaf_index)).unwrap(),
        )
    } else {
        (&StarknetStorageKey(Felt::from(leaf_index))).into()
    };
    assert_eq!(actual, expected_node_index);
}
