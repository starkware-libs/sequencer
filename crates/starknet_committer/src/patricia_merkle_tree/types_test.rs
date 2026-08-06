use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{contract_address_into_node_index, StarknetStorageKey};
use crate::patricia_merkle_tree::types::{
    fixed_hex_string_no_prefix,
    CompressedStateCommitmentInfos,
    StateCommitmentInfos,
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

/// Consumers one-shot decompress the payload, which requires the decompressed size to be readable
/// from the frame header rather than only by streaming the frame to its end.
#[test]
fn test_compressed_state_commitment_infos_frame_header_declares_decompressed_size() {
    let commitment_infos = StateCommitmentInfos::default();
    let bincode_payload_length = bincode::serialize(&commitment_infos).unwrap().len();

    let compressed = commitment_infos.compress().unwrap();

    assert_eq!(
        zstd::zstd_safe::get_frame_content_size(&compressed.0).unwrap(),
        Some(u64::try_from(bincode_payload_length).unwrap())
    );
    assert_eq!(compressed.decompress().unwrap(), commitment_infos);
}

/// Entries written before `compress` became one-shot carry no decompressed size in their frame
/// header; they must still decompress.
#[cfg(feature = "os_input")]
#[test]
fn test_decompress_frame_without_declared_decompressed_size() {
    let commitment_infos = StateCommitmentInfos::default();
    let streamed_frame = CompressedStateCommitmentInfos(
        zstd::encode_all(
            bincode::serialize(&commitment_infos).unwrap().as_slice(),
            zstd::DEFAULT_COMPRESSION_LEVEL,
        )
        .unwrap(),
    );

    assert_eq!(zstd::zstd_safe::get_frame_content_size(&streamed_frame.0).unwrap(), None);
    assert_eq!(streamed_frame.decompress().unwrap(), commitment_infos);
}

#[test]
fn test_compressed_state_commitment_infos_json_form_is_base64_string() {
    let compressed = CompressedStateCommitmentInfos(vec![40, 181, 47, 253, 0, 88]);
    let json_value = serde_json::to_value(&compressed).unwrap();
    assert_eq!(json_value, serde_json::Value::String("KLUv/QBY".to_string()));
    let roundtripped: CompressedStateCommitmentInfos = serde_json::from_value(json_value).unwrap();
    assert_eq!(roundtripped, compressed);
}
