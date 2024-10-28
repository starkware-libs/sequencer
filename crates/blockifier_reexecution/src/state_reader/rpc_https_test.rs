use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockNumber, StarknetVersion};
use starknet_api::class_hash;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::{DeclareTransaction, Transaction};
use starknet_core::types::ContractClass::{Legacy, Sierra};

use crate::state_reader::compile::legacy_to_contract_class_v0;
use crate::state_reader::test_state_reader::{ConsecutiveTestStateReaders, TestStateReader};

const EXAMPLE_INVOKE_TX_HASH: &str =
    "0xa7c7db686c7f756ceb7ca85a759caef879d425d156da83d6a836f86851983";

const EXAMPLE_BLOCK_NUMBER: u64 = 700000;

const EXAMPLE_CONTACT_CLASS_HASH: &str =
    "0x3131fa018d520a037686ce3efddeab8f28895662f019ca3ca18a626650f7d1e";

const EXAMPLE_DEPLOY_ACCOUNT_V1_BLOCK_NUMBER: u64 = 837408;
const EXAMPLE_DEPLOY_ACCOUNT_V1_TX_HASH: &str =
    "0x02a2e13cd94f911ea18c20a81e853314e37de58d49d13aa3a92370accd4338e8";

const EXAMPLE_DEPLOY_ACCOUNT_V3_BLOCK_NUMBER: u64 = 837792;
const EXAMPLE_DEPLOY_ACCOUNT_V3_TX_HASH: &str =
    "0x04422b1300d2a55fb0138f8a97819d6dc04fe1d57e332b657ce8167e6572c958";

const EXAMPLE_DECLARE_V1_BLOCK_NUMBER: u64 = 837461;
const EXAMPLE_DECLARE_V1_TX_HASH: &str =
    "0x04e9239ebc8512a508f21620cf570e9d938f31190770224d0f6d33ab93fefaf4";

const EXAMPLE_DECLARE_V2_BLOCK_NUMBER: u64 = 822636;
const EXAMPLE_DECLARE_V2_TX_HASH: &str =
    "0x0409d159fbcab271ffc1693b08d9198f4bbff7e344e1624dadc2d9a67a960226";

const EXAMPLE_DECLARE_V3_BLOCK_NUMBER: u64 = 825013;
const EXAMPLE_DECLARE_V3_TX_HASH: &str =
    "0x03ab43c0913f95b901b49ed1aa6009b31ebe0ad7ba62da49fc6de7f3854b496f";

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
pub fn test_get_invoke_tx_by_hash(test_state_reader: TestStateReader) {
    let actual_tx = test_state_reader.get_tx_by_hash(EXAMPLE_INVOKE_TX_HASH).unwrap();
    assert_matches!(actual_tx, Transaction::Invoke(..));
}

#[rstest(
    block_number,
    tx_hash,
    version,
    case(EXAMPLE_DEPLOY_ACCOUNT_V1_BLOCK_NUMBER, EXAMPLE_DEPLOY_ACCOUNT_V1_TX_HASH, 1),
    case(EXAMPLE_DEPLOY_ACCOUNT_V3_BLOCK_NUMBER, EXAMPLE_DEPLOY_ACCOUNT_V3_TX_HASH, 3)
)]
pub fn test_get_deploy_account_tx_by_hash(block_number: u64, tx_hash: &str, version: u64) {
    // Create StateReader with block number that contain the deploy account tx.
    use starknet_api::transaction::DeployAccountTransaction;
    let state_reader = TestStateReader::new_for_testing(BlockNumber(block_number));
    let actual_tx = state_reader.get_tx_by_hash(tx_hash).unwrap();
    match version {
        1 => {
            assert_matches!(actual_tx, Transaction::DeployAccount(DeployAccountTransaction::V1(..)))
        }
        3 => {
            assert_matches!(actual_tx, Transaction::DeployAccount(DeployAccountTransaction::V3(..)))
        }
        _ => panic!("Invalid version"),
    }
}

#[rstest(
    block_number,
    tx_hash,
    expected_version,
    case(EXAMPLE_DECLARE_V1_BLOCK_NUMBER, EXAMPLE_DECLARE_V1_TX_HASH, 1),
    case(EXAMPLE_DECLARE_V2_BLOCK_NUMBER, EXAMPLE_DECLARE_V2_TX_HASH, 2),
    case(EXAMPLE_DECLARE_V3_BLOCK_NUMBER, EXAMPLE_DECLARE_V3_TX_HASH, 3)
)]
pub fn test_get_declare_tx_by_hash(block_number: u64, tx_hash: &str, expected_version: u64) {
    // Create StateReader with block number that contain the declare tx.
    let state_reader = TestStateReader::new_for_testing(BlockNumber(block_number));
    let actual_tx = state_reader.get_tx_by_hash(tx_hash).unwrap();
    match expected_version {
        1 => assert_matches!(actual_tx, Transaction::Declare(DeclareTransaction::V1(..))),
        2 => assert_matches!(actual_tx, Transaction::Declare(DeclareTransaction::V2(..))),
        3 => assert_matches!(actual_tx, Transaction::Declare(DeclareTransaction::V3(..))),
        _ => panic!("Invalid expected version"),
    }
}

#[rstest]
pub fn test_get_statediff_rpc(test_state_reader: TestStateReader) {
    assert!(test_state_reader.get_state_diff().is_ok());
}

// TODO(Aner): replace this test with a CLI command that receives the node URL as input.
#[rstest]
pub fn test_full_blockifier_via_rpc(
    test_state_readers_last_and_current_block: ConsecutiveTestStateReaders,
) {
    let all_txs_in_next_block =
        test_state_readers_last_and_current_block.get_next_block_txs().unwrap();

    let mut expected_state_diff =
        test_state_readers_last_and_current_block.get_next_block_state_diff().unwrap();

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
