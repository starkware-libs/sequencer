use apollo_starknet_client::reader::objects::block::BlockStatus;
use rstest::rstest;
use starknet_api::block::BlockNumber;

use crate::serialization::to_python_json;
use crate::status::finalized_block_status;

#[rstest]
#[case::below_marker_is_accepted_on_l1(BlockNumber(4), BlockNumber(5), BlockStatus::AcceptedOnL1)]
#[case::at_marker_is_accepted_on_l2(BlockNumber(5), BlockNumber(5), BlockStatus::AcceptedOnL2)]
#[case::above_marker_is_accepted_on_l2(BlockNumber(6), BlockNumber(5), BlockStatus::AcceptedOnL2)]
#[case::genesis_with_zero_marker_is_accepted_on_l2(
    BlockNumber(0),
    BlockNumber(0),
    BlockStatus::AcceptedOnL2
)]
#[case::genesis_with_advanced_marker_is_accepted_on_l1(
    BlockNumber(0),
    BlockNumber(1),
    BlockStatus::AcceptedOnL1
)]
fn finalized_block_status_against_base_layer_marker(
    #[case] block_number: BlockNumber,
    #[case] base_layer_marker: BlockNumber,
    #[case] expected_status: BlockStatus,
) {
    assert_eq!(finalized_block_status(block_number, base_layer_marker), expected_status);
}

#[test]
fn block_status_serializes_to_legacy_strings() {
    // Locks the wire strings the status computation feeds into responses.
    assert_eq!(to_python_json(&BlockStatus::AcceptedOnL1).unwrap(), r#""ACCEPTED_ON_L1""#);
    assert_eq!(to_python_json(&BlockStatus::AcceptedOnL2).unwrap(), r#""ACCEPTED_ON_L2""#);
}
