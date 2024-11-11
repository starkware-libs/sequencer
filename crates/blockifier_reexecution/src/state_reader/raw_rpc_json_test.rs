use std::collections::HashMap;

use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use blockifier::state::cached_state::StateMaps;
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
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, nonce, storage_key};
use starknet_core::types::ContractClass;
use starknet_gateway::rpc_objects::BlockHeader;

use crate::state_reader::compile::legacy_to_contract_class_v0;
use crate::state_reader::serde_utils::deserialize_transaction_json_to_starknet_api_tx;
use crate::state_reader::utils::{reexecute_block_for_testing, ReexecutionStateMaps};

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
    let deploy_account = deserialize_transaction_json_to_starknet_api_tx(
        read_json_file("raw_rpc_json_objects/transactions.json")[deploy_account_version].clone(),
    )
    .unwrap_or_else(|_| panic!("Failed to deserialize {deploy_account_version} tx"));

    match deploy_account_version {
        "deploy_account_v1" => {
            assert_matches!(
                deploy_account,
                Transaction::DeployAccount(DeployAccountTransaction::V1(..))
            )
        }
        "deploy_account_v3" => {
            assert_matches!(
                deploy_account,
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

#[test]
fn deserialize_l1_handler_tx() {
    let l1_handler_tx = deserialize_transaction_json_to_starknet_api_tx(
        read_json_file("raw_rpc_json_objects/transactions.json")["l1_handler"].clone(),
    )
    .expect("Failed to deserialize l1 handler tx");

    assert_matches!(l1_handler_tx, Transaction::L1Handler(..));
}

#[rstest]
fn serialize_state_maps() {
    let nonces = HashMap::from([(contract_address!(1_u8), nonce!(1_u8))]);
    let class_hashes = HashMap::from([(contract_address!(1_u8), class_hash!(27_u8))]);
    let compiled_class_hashes = HashMap::from([(class_hash!(27_u8), compiled_class_hash!(27_u8))]);
    let declared_contracts = HashMap::from([(class_hash!(27_u8), true)]);
    let storage = HashMap::from([
        ((contract_address!(1_u8), storage_key!(27_u8)), felt!(1_u8)),
        ((contract_address!(30_u8), storage_key!(27_u8)), felt!(2_u8)),
        ((contract_address!(30_u8), storage_key!(28_u8)), felt!(3_u8)),
    ]);

    let original_state_maps =
        StateMaps { nonces, class_hashes, storage, compiled_class_hashes, declared_contracts };

    let serializable_state_maps = ReexecutionStateMaps::from(original_state_maps.clone());

    // Check that the created statemaps can be serialized.
    let json = serde_json::to_string_pretty(&serializable_state_maps)
        .expect("Failed to serialize state maps");

    let deserialized_state_maps: ReexecutionStateMaps =
        serde_json::from_str(&json).expect("Failed to deserialize state maps");

    assert_eq!(serializable_state_maps, deserialized_state_maps);
    assert_eq!(original_state_maps, deserialized_state_maps.try_into().unwrap());
}

#[rstest]
// TODO(Aner): Add block for each starknet version and for declare, deploy, replace_class, etc.
// #[case::v_0_13_0(600001)]
// #[case::v_0_13_1(620978)]
// #[case::v_0_13_1_1(649367)]
// #[case::v_0_13_2(685878)]
// #[case::v_0_13_2_1(700000)]
// #[case::invoke_with_replace_class_syscall(780008)]
// #[case::invoke_with_deploy_syscall(870136)]
// #[case::example_deploy_account_v1(837408)]
// #[case::example_deploy_account_v3(837792)]
#[case::example_declare_v1(837461)]
// #[case::example_declare_v2(822636)]
#[case::example_declare_v3(825013)]
// #[case::example_l1_handler(868429)]
#[ignore = "Requires downloading JSON files prior to running; Long test, run with --release flag."]
fn test_block_reexecution(#[case] block_number: u64) {
    reexecute_block_for_testing(block_number);
}
