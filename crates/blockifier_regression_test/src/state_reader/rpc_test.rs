use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use blockifier::versioned_constants::StarknetVersion;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;

use crate::state_reader::test_state_reader::TestStateReader;

#[fixture]
pub fn test_block_number() -> BlockNumber {
    BlockNumber(700000)
}

#[fixture]
pub fn test_state_reader(test_block_number: BlockNumber) -> TestStateReader {
    TestStateReader::new_for_testing(test_block_number)
}

#[rstest]
pub fn test_get_block_info(test_state_reader: TestStateReader, test_block_number: BlockNumber) {
    assert_matches!(
        test_state_reader.get_block_info(),
        Ok(BlockInfo { block_number, .. }) if block_number == test_block_number
    );
}

#[rstest]
pub fn test_get_starknet_version(test_state_reader: TestStateReader) {
    assert_eq!(test_state_reader.get_starknet_version().unwrap(), StarknetVersion::V0_13_2_1)
}
