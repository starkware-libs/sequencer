use rstest::rstest;
use starknet_patricia::felt::Felt;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;

use crate::block_committer::input::{ContractAddress, StarknetStorageKey};

#[rstest]
fn test_cast_to_node_index(
    #[values(0, 15, 0xDEADBEEF)] leaf_index: u128,
    #[values(true, false)] bool_from_contract_address: bool,
) {
    let expected_node_index = NodeIndex::FIRST_LEAF + leaf_index;
    let actual: NodeIndex = if bool_from_contract_address {
        (&ContractAddress(Felt::from(leaf_index))).into()
    } else {
        (&StarknetStorageKey(Felt::from(leaf_index))).into()
    };
    assert_eq!(actual, expected_node_index);
}
