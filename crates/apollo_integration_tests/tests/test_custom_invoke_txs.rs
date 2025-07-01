use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    ACCOUNT_ID_0 as CAIRO1_ACCOUNT_ID,
    N_TXS_IN_NON_GENERIC_INVOKE_TXS,
};
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    AccountId,
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::felt;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::common::{end_to_end_flow, TestScenario};

mod common;

const DEFAULT_TIP: u64 = 1_u64;

#[tokio::test]
async fn all_custom_invoke_txs() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestCustomInvokeTxs,
        create_custom_invoke_txs_scenario(),
        GasAmount(60000000),
        false,
        false,
    )
    .await
}

fn create_custom_invoke_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_custom_invoke_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_custom_invoke_txs_hashes,
    }]
}

pub fn test_custom_invoke_txs_hashes(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert!(
        tx_hashes.len() == N_TXS_IN_NON_GENERIC_INVOKE_TXS,
        "Unexpected number of transactions sent in the test scenario. Found {} transactions",
        tx_hashes.len()
    );
    tx_hashes.to_vec()
}

fn create_custom_invoke_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    create_cairo_1_syscall_test_txs(tx_generator, CAIRO1_ACCOUNT_ID)
}

/// Creates a set of transactions that test the Cairo 1.0 syscall functionality.
/// The transaction are taken from: https://github.com/starkware-industries/starkware/blob/dev/src/starkware/starknet/services/utils/deprecated_test_utils.py#L1601
pub fn create_cairo_1_syscall_test_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
    account_id: AccountId,
) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(account_id);
    let test_contract = FeatureContract::TestContract(account_tx_generator.account.cairo_version());

    let mut txs = vec![];
    txs.extend(generate_custom_library_call_invoke_txs(account_tx_generator, &test_contract));
    txs.extend(generate_custom_not_nested_invoke_txs(account_tx_generator, &test_contract));

    txs
}

fn generate_custom_not_nested_invoke_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
    test_contract: &FeatureContract,
) -> Vec<RpcTransaction> {
    let test_deploy_args = vec![
        test_contract.get_class_hash().0, // class hash
        felt!(7_u64),                     // salt
        felt!(2_u64),                     // len of construct calldata.
        felt!(1_u64),                     // construct calldata: arg1.
        felt!(1_u64),                     // construct calldata: arg2.
        felt!(0_u64),                     // deploy_from_zero flag is down.
    ];
    let test_send_message_to_l1_args = vec![
        felt!(0_u64),    // target address
        felt!(2_u8),     // payload length
        felt!(4365_u64), // payload 1
        felt!(23_u64),   // payload 2
    ];
    let test_emit_events_args = vec![
        felt!(2_u64),    // number of arguments
        felt!(1_u64),    // key length
        felt!(2991_u64), // key
        felt!(2_u64),    // value length
        felt!(42_u64),   // value 1
        felt!(153_u64),  // value 2
    ];
    let test_keccak_args = vec![];

    [
        ("test_deploy", test_deploy_args),
        ("test_send_message_to_l1", test_send_message_to_l1_args),
        ("test_emit_events", test_emit_events_args),
        ("test_keccak", test_keccak_args),
    ]
    .iter()
    .map(|(fn_name, fn_args)| {
        account_tx_generator.generate_generic_rpc_invoke_tx(
            DEFAULT_TIP,
            fn_name,
            fn_args,
            test_contract.get_instance_address(0),
        )
    })
    .collect()
}

fn generate_custom_library_call_invoke_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
    test_contract: &FeatureContract,
) -> Vec<RpcTransaction> {
    // Define the arguments for the library calls.
    let test_storage_read_write_args = vec![
        felt!(2_u8),     // number of arguments.
        felt!(1948_u64), // key.
        felt!(1967_u64), // value.
    ];
    let test_sha256_args = vec![felt!(0_u64)]; // No arguments for test_sha256.
    let test_circuit_args = vec![felt!(0_u64)]; // No arguments for test_circuit.

    // Generate the invoke transactions for each library call.
    [
        ("test_storage_read_write", test_storage_read_write_args),
        ("test_sha256", test_sha256_args),
        ("test_circuit", test_circuit_args),
    ]
    .iter()
    .map(|(fn_name, fn_args)| {
        account_tx_generator.generate_invoke_tx_library_call(DEFAULT_TIP, fn_name, fn_args, test_contract)
    })
    .collect()
}
