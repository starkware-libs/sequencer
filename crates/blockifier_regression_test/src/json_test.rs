use std::fs::File;
use std::io::BufReader;

use blockifier::blockifier::block::BlockInfo;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_gateway::rpc_objects::BlockHeader;

#[fixture]
fn block_header() -> BlockHeader {
    let file_path = "src/json_objects/block_header.json";
    let file = File::open(file_path).expect("Failed to open the file");
    let reader = BufReader::new(file);

    // Deserialize the JSON data into a BlockInfo struct
    serde_json::from_reader(reader).expect("Failed to deserialize JSON")
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
