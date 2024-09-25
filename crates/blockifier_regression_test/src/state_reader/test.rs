use std::fs::File;

use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use blockifier::state::state_api::StateReader;
use blockifier::versioned_constants::StarknetVersion;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::transaction::Transaction;
use starknet_api::{class_hash, felt};

use crate::state_reader::test_state_reader::TestStateReader;

#[fixture]
pub fn test_state_reader(test_block_number: BlockNumber) -> TestStateReader {
    TestStateReader::new_for_testing(test_block_number)
}

#[fixture]
pub fn test_block_number() -> BlockNumber {
    BlockNumber(700000)
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
    assert!(test_state_reader.get_starknet_version().unwrap() == StarknetVersion::V0_13_2_1)
}

#[rstest]
pub fn test_get_contract_class(test_state_reader: TestStateReader) {
    let class_hash =
        class_hash!("0x3131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e");
    test_state_reader.get_contract_class(&class_hash).unwrap_or_else(|err| {
        panic!("Error retrieving contract class for class hash {}: {}", class_hash, err);
    });
}

#[rstest]
pub fn test_get_compiled_contract_class(test_state_reader: TestStateReader) {
    let class_hash =
        class_hash!("0x3131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e");
    test_state_reader.get_compiled_contract_class(class_hash).unwrap_or_else(|err| {
        panic!("Error retrieving compiled contract class for class hash {}: {}", class_hash, err);
    });
}

#[rstest]
pub fn test_get_txs_hash(test_state_reader: TestStateReader) {
    let raw_txs_hash = File::open("./src/data/txs_hash_block_700000.json").unwrap();
    let txs_hash: Vec<String> = serde_json::from_reader(raw_txs_hash).unwrap();
    let actual_tx_hash = test_state_reader.get_txs_hash().unwrap();
    assert!(actual_tx_hash == txs_hash);
}

#[rstest]
pub fn test_get_tx_by_hash(test_state_reader: TestStateReader) {
    let tx_hash = "0x47165a9a9c97e8829a4778f2a4b6fae4366aefc35b51d484bf04c458168351b";
    let actual_tx = test_state_reader.get_txs_by_hash(tx_hash).unwrap();
    assert_matches!(actual_tx, Transaction::Invoke(..));
}
