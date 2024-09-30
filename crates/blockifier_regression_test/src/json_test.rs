use blockifier::blockifier::block::BlockInfo;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::test_utils::read_json_file;
use starknet_gateway::rpc_objects::BlockHeader;

#[fixture]
fn block_header() -> BlockHeader {
    serde_json::from_value(read_json_file("json_objects/block_header.json"))
        .expect("Failed to deserialize block header")
}

/// Test that deserialize block header from JSON file works(in the fixture).
#[rstest]
fn test_deserialize_block_header(block_header: BlockHeader) {
    assert_eq!(block_header.block_number, BlockNumber(700000));
}

/// Test that converting a block header to block info works.
#[rstest]
fn test_block_header_to_block_info(block_header: BlockHeader) {
    let block_info: BlockInfo =
        block_header.try_into().expect("Failed to convert BlockHeader to block info");
    // Sanity check.
    assert_eq!(block_info.block_number, BlockNumber(700000));
}
