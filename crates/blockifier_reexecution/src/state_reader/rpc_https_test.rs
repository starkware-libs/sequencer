use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use blockifier::versioned_constants::StarknetVersion;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::Transaction;
use starknet_api::{class_hash, felt};
use starknet_core::types::ContractClass::{Legacy, Sierra};

use crate::state_reader::compile::legacy_to_contract_class_v0;
use crate::state_reader::test_state_reader::{ConsecutiveTestStateReaders, TestStateReader};

const EXAMPLE_INVOKE_TX_HASH: &str =
    "0xa7c7db686c7f756ceb7ca85a759caef879d425d156da83d6a836f86851983";

const EXAMPLE_BLOCK_NUMBER: u64 = 700000;

const EXAMPLE_CONTACT_CLASS_HASH: &str =
    "0x3131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e";

#[fixture]
pub fn test_block_number() -> BlockNumber {
    BlockNumber(EXAMPLE_BLOCK_NUMBER)
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

#[fixture]
pub fn last_constructed_block() -> BlockNumber {
    BlockNumber(EXAMPLE_BLOCK_NUMBER - 1)
}

#[fixture]
pub fn test_state_readers_last_and_current_block(
    last_constructed_block: BlockNumber,
) -> ConsecutiveTestStateReaders {
    ConsecutiveTestStateReaders::new(last_constructed_block, None)
}

#[rstest]
pub fn test_get_starknet_version(test_state_reader: TestStateReader) {
    assert_eq!(test_state_reader.get_starknet_version().unwrap(), StarknetVersion::V0_13_2_1)
}

#[rstest]
pub fn test_get_contract_class(test_state_reader: TestStateReader, test_block_number: BlockNumber) {
    // An example of existing class hash in Mainnet.
    let class_hash = class_hash!(EXAMPLE_CONTACT_CLASS_HASH);

    // Test getting the contract class using RPC request.
    let deprecated_contract_class =
        test_state_reader.get_contract_class(&class_hash).unwrap_or_else(|err| {
            panic!(
                "Error retrieving deprecated contract class for class hash {}: {}
            This class hash exist in Mainnet Block Number: {}",
                class_hash, test_block_number, err
            );
        });

    // Test compiling the contract class.
    match deprecated_contract_class {
        Legacy(legacy) => {
            // Test compiling the contract class.
            assert!(legacy_to_contract_class_v0(legacy).is_ok());
        }
        // This contract class is deprecated.
        Sierra(_) => panic!("Expected a legacy contract class"),
    }
}

#[rstest]
pub fn test_get_tx_hashes(test_state_reader: TestStateReader, test_block_number: BlockNumber) {
    let block_number_int = test_block_number.0;
    let expected_tx_hashes: Vec<String> = serde_json::from_value(read_json_file(
        format!("block_{block_number_int}/tx_hashes_block_{block_number_int}.json").as_str(),
    ))
    .unwrap_or_else(|err| panic!("Failed to deserialize txs hash to Vector of String {}", err));
    let actual_tx_hashes = test_state_reader.get_tx_hashes().unwrap_or_else(|err| {
        panic!("Error retrieving txs hash: {}", err);
    });
    assert_eq!(actual_tx_hashes, expected_tx_hashes);
}

#[rstest]
pub fn test_get_tx_by_hash(test_state_reader: TestStateReader) {
    let actual_tx = test_state_reader.get_tx_by_hash(EXAMPLE_INVOKE_TX_HASH).unwrap();
    assert_matches!(actual_tx, Transaction::Invoke(..));
}

#[rstest]
pub fn test_get_statediff_rpc(test_state_reader: TestStateReader) {
    assert!(test_state_reader.get_state_diff().is_ok());
}

// TODO(Aner): remove test and add as CLI test
#[rstest]
pub fn test_full_blockifier_via_rpc(
    test_state_readers_last_and_current_block: ConsecutiveTestStateReaders,
) {
    let all_txs_in_next_block = test_state_readers_last_and_current_block.get_txs().unwrap();

    let mut expected_state_diff =
        test_state_readers_last_and_current_block.get_state_diff().unwrap();

    let mut transaction_executor =
        test_state_readers_last_and_current_block.get_transaction_executor(None).unwrap();

    transaction_executor.execute_txs(&all_txs_in_next_block);
    // Finalize block and read actual statediff.
    let (actual_state_diff, _, _) =
        transaction_executor.finalize().expect("Couldn't finalize block");
    // TODO(Aner): compute the correct block hash at storage slot 0x1 instead of removing it.
    expected_state_diff.storage_updates.shift_remove(&ContractAddress(1_u128.into()));
    assert_eq!(expected_state_diff, actual_state_diff);
}
