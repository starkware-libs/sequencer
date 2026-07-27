use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{contract_address_into_node_index, StarknetStorageKey};
use crate::patricia_merkle_tree::types::{
    fixed_hex_string_no_prefix,
    CompressedStateCommitmentInfos,
};

#[rstest]
fn test_cast_to_node_index(
    #[values(0, 15, 0xDEADBEEF)] leaf_index: u128,
    #[values(true, false)] bool_from_contract_address: bool,
) {
    let expected_node_index = NodeIndex::FIRST_LEAF + leaf_index;
    let actual: NodeIndex = if bool_from_contract_address {
        contract_address_into_node_index(
            &ContractAddress::try_from(Felt::from(leaf_index)).unwrap(),
        )
    } else {
        (&StarknetStorageKey::from(leaf_index)).into()
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

#[test]
fn test_compressed_state_commitment_infos_json_form_is_base64_string() {
    let compressed = CompressedStateCommitmentInfos(vec![40, 181, 47, 253, 0, 88]);
    let json_value = serde_json::to_value(&compressed).unwrap();
    assert_eq!(json_value, serde_json::Value::String("KLUv/QBY".to_string()));
    let roundtripped: CompressedStateCommitmentInfos = serde_json::from_value(json_value).unwrap();
    assert_eq!(roundtripped, compressed);
}
