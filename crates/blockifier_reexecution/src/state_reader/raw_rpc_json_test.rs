use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::{
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    Transaction,
};
use starknet_core::types::ContractClass;
use starknet_gateway::rpc_objects::BlockHeader;

use crate::state_reader::compile::legacy_to_contract_class_v0;
use crate::state_reader::serde_utils::deserialize_transaction_json_to_starknet_api_tx;

#[fixture]
fn block_header() -> BlockHeader {
    serde_json::from_value(read_json_file("raw_rpc_json_objects/block_header.json"))
        .expect("Failed to deserialize block header")
}

#[fixture]
fn deprecated_contract_class() -> ContractClass {
    serde_json::from_value(read_json_file("raw_rpc_json_objects/deprecated_contract_class.json"))
        .expect("Failed to deserialize deprecated contact class")
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

#[rstest]
fn test_compile_deprecated_contract_class(deprecated_contract_class: ContractClass) {
    match deprecated_contract_class {
        ContractClass::Legacy(legacy) => {
            // Compile the contract class.
            assert!(legacy_to_contract_class_v0(legacy).is_ok());
        }
        _ => panic!("Expected a legacy contract class"),
    }
}

#[test]
fn deserialize_invoke_txs() {
    let invoke_tx_v1 = deserialize_transaction_json_to_starknet_api_tx(
        read_json_file("raw_rpc_json_objects/transactions.json")["invoke_v1"].clone(),
    )
    .expect("Failed to deserialize invoke v1 tx");

    assert_matches!(invoke_tx_v1, Transaction::Invoke(InvokeTransaction::V1(..)));

    let invoke_tx_v3 = deserialize_transaction_json_to_starknet_api_tx(
        read_json_file("raw_rpc_json_objects/transactions.json")["invoke_v3"].clone(),
    )
    .expect("Failed to deserialize invoke v3 tx");

    assert_matches!(invoke_tx_v3, Transaction::Invoke(InvokeTransaction::V3(..)));
}

#[rstest]
fn deserialize_deploy_account_txs(
    #[values("deploy_account_v1", "deploy_account_v3")] deploy_account_version: &str,
) {
    let deploy_account_tx_v1 = deserialize_transaction_json_to_starknet_api_tx(
        read_json_file("raw_rpc_json_objects/transactions.json")[deploy_account_version].clone(),
    )
    .unwrap_or_else(|_| panic!("Failed to deserialize {deploy_account_version} tx"));

    match deploy_account_version {
        "deploy_account_v1" => {
            assert_matches!(
                deploy_account_tx_v1,
                Transaction::DeployAccount(DeployAccountTransaction::V1(..))
            )
        }
        "deploy_account_v3" => {
            assert_matches!(
                deploy_account_tx_v1,
                Transaction::DeployAccount(DeployAccountTransaction::V3(..))
            )
        }
        _ => panic!("Unknown scenario '{deploy_account_version}'"),
    }
}

#[rstest]
fn deserialize_declare_txs(
    #[values("declare_v1", "declare_v2", "declare_v3")] declare_version: &str,
) {
    let declare_tx = deserialize_transaction_json_to_starknet_api_tx(
        read_json_file("raw_rpc_json_objects/transactions.json")[declare_version].clone(),
    )
    .unwrap_or_else(|_| panic!("Failed to deserialize {declare_version} tx"));

    match declare_version {
        "declare_v1" => {
            assert_matches!(declare_tx, Transaction::Declare(DeclareTransaction::V1(..)))
        }
        "declare_v2" => {
            assert_matches!(declare_tx, Transaction::Declare(DeclareTransaction::V2(..)))
        }
        "declare_v3" => {
            assert_matches!(declare_tx, Transaction::Declare(DeclareTransaction::V3(..)))
        }
        _ => panic!("Unknown scenario '{declare_version}'"),
    }
}
