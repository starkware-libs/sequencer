use rstest::rstest;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{ContractAddress, StarknetStorageKey};
use crate::patricia_merkle_tree::types::fixed_hex_string_no_prefix;

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

#[rstest]
fn test_fixed_hex_string_no_prefix(
    #[values(Felt::ZERO, Felt::ONE, Felt::MAX, Felt::from(u128::MAX))] value: Felt,
) {
    let fixed_hex = fixed_hex_string_no_prefix(&value);
    assert_eq!(fixed_hex.len(), 64);
    assert_eq!(Felt::from_hex(&fixed_hex).unwrap(), value);
}
