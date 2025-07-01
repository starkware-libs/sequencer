use std::sync::LazyLock;

use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::ACCOUNT_ID_0 as CAIRO1_ACCOUNT_ID;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use starknet_api::core::{calculate_contract_address, CompiledClassHash};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::{calldata, felt};

use crate::common::{end_to_end_flow, validate_tx_count, TestScenario};

mod common;

const DEFAULT_TIP: u64 = 1_u64;
const CUSTOM_INVOKE_TX_COUNT: usize = 10;

/// Test a wide range of different kinds of invoke transactions.
#[tokio::test]
async fn custom_syscall_invoke_txs() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestCustomInvokeTxs,
        create_custom_syscall_invoke_txs_scenario(),
        GasAmount(90000000),
        false,
        false,
    )
    .await
}

fn create_custom_syscall_invoke_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_cairo_1_syscall_test_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: |tx_hashes| {
            validate_tx_count(tx_hashes, CUSTOM_INVOKE_TX_COUNT, "Custom invoke transactions test")
        },
    }]
}

/// Creates a set of transactions that test the Cairo 1.0 syscall functionality.
/// The transaction are taken from: https://github.com/starkware-industries/starkware/blob/dev/src/starkware/starknet/services/utils/deprecated_test_utils.py#L1601
fn create_cairo_1_syscall_test_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(CAIRO1_ACCOUNT_ID);
    let mut txs = vec![];
    txs.push(generate_empty_contract_declare_tx(account_tx_generator));
    txs.extend(generate_custom_library_call_invoke_txs(account_tx_generator));
    txs.extend(generate_custom_not_nested_invoke_txs(account_tx_generator));
    txs.extend(generate_test_deploy_txs(account_tx_generator, DEFAULT_TIP));

    txs
}

/// Generates invoke txs which calls functions directly from the test contract.
fn generate_custom_not_nested_invoke_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
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
        ("test_send_message_to_l1", test_send_message_to_l1_args),
        ("test_emit_events", test_emit_events_args),
        ("test_keccak", test_keccak_args),
    ]
    .iter()
    .map(|(fn_name, fn_args)| {
        let calldata = create_calldata(test_contract.get_instance_address(0), fn_name, fn_args);
        account_tx_generator.generate_rpc_invoke_tx(DEFAULT_TIP, calldata)
    })
    .collect()
}

fn generate_custom_library_call_invoke_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    // Define the arguments for the library calls.
    let test_sha256_args = vec![felt!(0_u64)]; // No arguments for test_sha256.
    let test_circuit_args = vec![felt!(0_u64)]; // No arguments for test_circuit.

    // Generate the invoke transactions for each library call.
    [("test_sha256", test_sha256_args), ("test_circuit", test_circuit_args)]
        .iter()
        .map(|(fn_name, fn_args)| {
            account_tx_generator.generate_invoke_tx_library_call(
                DEFAULT_TIP,
                fn_name,
                fn_args,
                &test_contract,
            )
        })
        .collect()
}

fn generate_empty_contract_declare_tx(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> RpcTransaction {
    // TODO(Itamar): Consider changing the empty contract to another contract with more functions and check class.
    let empty_contract = FeatureContract::Empty(account_tx_generator.account.cairo_version());
    // Hard coded class hash for empty contract. See example in https://github.com/starkware-libs/sequencer/blob/main-v0.14.0/crates/mempool_test_utils/src/starknet_api_test_utils.rs#L92
    // TODO(Itamar): Move compiled hash to the blockifier constants file as optional trait for
    // FeatureContract.
    let empty_compiled_class_hash = CompiledClassHash(felt!(
        "0x317D3AC2CF840E487B6D0014A75F0CF507DFF0BC143C710388E323487089BFA"
    ));

    account_tx_generator.generate_declare_tx(empty_compiled_class_hash, empty_contract.get_sierra())
}

fn generate_test_deploy_txs(
    account_tx_generator: &mut AccountTransactionGenerator,
    tip: u64,
) -> Vec<RpcTransaction> {
    let mut txs = vec![];
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // test_deploy_contract - constructor args and salt are unique to calculate contract address.
    let constructor_calldata_arg1 = felt!(1_u8);
    let constructor_calldata_arg2 = felt!(1_u8);
    let salt_deployed_account = felt!(7_u64);
    let test_deploy_args = vec![
        test_contract.get_class_hash().0, // class hash
        salt_deployed_account,            // salt
        felt!(2_u64),                     // len of construct calldata.
        constructor_calldata_arg1,        // construct calldata: arg1.
        constructor_calldata_arg2,        // construct calldata: arg2.
        felt!(0_u64),                     // deploy_from_zero flag is down.
    ];
    let calldata =
        create_calldata(test_contract.get_instance_address(0), "test_deploy", &test_deploy_args);
    txs.push(account_tx_generator.generate_rpc_invoke_tx(DEFAULT_TIP, calldata));

    // Get the contract address of the newly deployed contract from test_deploy.
    let newly_deployed_contract_address = calculate_contract_address(
        ContractAddressSalt(salt_deployed_account),
        test_contract.get_class_hash(),
        &calldata!(constructor_calldata_arg1, constructor_calldata_arg2), /* constructor
                                                                           * calldata */
        test_contract.get_instance_address(0), // deployer address
    )
    .expect("Failed to calculate contract address");

    // Write key and value to storage via test_call_contract of the deployed contract.
    let key = felt!(1948_u64);
    let test_storage_write_args = &[
        felt!(2_u64),    // arguments length
        key,             // key
        felt!(1967_u64), // value
    ];

    txs.push(account_tx_generator.generate_call_contract_invoke_tx(
        newly_deployed_contract_address,
        &test_contract,
        "test_storage_write",
        test_storage_write_args,
        tip,
    ));

    // Read value by key from storage via test_library_call of the deployed contract.
    let test_storage_read_args = vec![
        felt!(1_u64), // arguments length
        key,
    ];
    txs.push(account_tx_generator.generate_invoke_tx_library_call(
        DEFAULT_TIP,
        "test_storage_read",
        &test_storage_read_args,
        &test_contract,
    ));

    // test_replace_class - replace the class of the deployed contract with an empty contract.
    let empty_contract = FeatureContract::Empty(account_tx_generator.account.cairo_version());
    let test_replace_class_args =
        vec![empty_contract.safe_get_sierra().unwrap().calculate_class_hash().0];
    let calldata = create_calldata(
        newly_deployed_contract_address,
        "test_replace_class",
        &test_replace_class_args,
    );

    txs.push(account_tx_generator.generate_rpc_invoke_tx(DEFAULT_TIP, calldata));

    txs
}
