use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::ACCOUNT_ID_1 as CAIRO0_ACCOUNT_ID;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use starknet_api::core::calculate_contract_address;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::{ContractAddressSalt, TransactionSignature};
use starknet_api::{calldata, felt};

use crate::common::{end_to_end_flow, validate_tx_count, TestScenario};

mod common;

const CUSTOM_CAIRO_0_INVOKE_TX_COUNT: usize = 9;

/// The test uses 3 threads: 1 for the test's main thread and 2 for the sequencers.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn custom_cairo0_txs() {
    end_to_end_flow(
        TestIdentifier::EndToEndFlowTestCustomCairo0Txs,
        create_custom_cairo0_txs_scenario(),
        GasAmount(110000000),
        false,
        false,
    )
    .await
}

fn create_custom_cairo0_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_custom_cairo0_test_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: |tx_hashes| validate_tx_count(tx_hashes, CUSTOM_CAIRO_0_INVOKE_TX_COUNT),
    }]
}

/// Creates a set of transactions that test the Cairo 0 functionality.
fn create_custom_cairo0_test_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(CAIRO0_ACCOUNT_ID);
    let mut txs = vec![];
    txs.extend(generate_direct_test_contract_invoke_txs_cairo_0_syscall(account_tx_generator));
    txs.extend(generate_invoke_txs_with_signature_cairo_0_syscall(account_tx_generator));
    txs.extend(generate_invoke_txs_tests_for_deploy_contract(account_tx_generator));

    txs
}

fn generate_direct_test_contract_invoke_txs_cairo_0_syscall(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);

    [
        (
            "advance_counter",
            vec![
                felt!(2021_u64), // index
                felt!(7_u64),    // diff_0
                felt!(7_u64),    // diff_1
            ],
        ),
        (
            "xor_counters",
            vec![
                felt!(2021_u64), // index
                felt!(31_u64),   // values.x
                felt!(21_u64),   // values.y
            ],
        ),
        ("test_ec_op", vec![]),
    ]
    .iter()
    .map(|(fn_name, fn_args)| {
        let calldata = create_calldata(test_contract.get_instance_address(0), fn_name, fn_args);
        rpc_invoke_tx(account_tx_generator.build_invoke_tx_args().calldata(calldata))
    })
    .collect()
}

fn generate_invoke_txs_with_signature_cairo_0_syscall(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);

    let fn_name = "add_signature_to_counters";
    let fn_args = vec![felt!(2021_u64)];
    let calldata = create_calldata(test_contract.get_instance_address(0), fn_name, &fn_args);

    let signature = TransactionSignature(vec![felt!(100_u64), felt!(200_u64)].into());

    vec![rpc_invoke_tx(
        account_tx_generator.build_invoke_tx_args().calldata(calldata).signature(signature),
    )]
}

// Deploy a contract and test with 4 more transactions the deployed contract functionality.
fn generate_invoke_txs_tests_for_deploy_contract(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let mut txs = vec![];
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let salt = felt!(34_u64);
    let constructor_calldata_arg1 = felt!(321_u64);
    let constructor_calldata_arg2 = felt!(543_u64);

    // deploy_contract - deploy the test contract.
    let deploy_calldata = vec![
        test_contract.get_class_hash().0, // class hash for test contract
        salt,                             // salt
        felt!(2_u64),                     // constructor calldata length
        constructor_calldata_arg1,        // constructor arg1
        constructor_calldata_arg2,        // constructor arg2
    ];
    let calldata =
        create_calldata(account_tx_generator.sender_address(), "deploy_contract", &deploy_calldata);
    txs.push(rpc_invoke_tx(account_tx_generator.build_invoke_tx_args().calldata(calldata)));

    // Get the contract address of the newly deployed contract from deploy_contract.
    let newly_deployed_contract_address = calculate_contract_address(
        ContractAddressSalt(salt),
        test_contract.get_class_hash(),
        &calldata!(constructor_calldata_arg1, constructor_calldata_arg2),
        account_tx_generator.sender_address(),
    )
    .expect("Failed to calculate contract address");

    // 1. test_call_storage_consistency - Change storage of newly deployed contract via
    //    test_contract and verify it.
    let test_call_storage_consistency_args = vec![
        *newly_deployed_contract_address.0.key(),
        felt!(5_u64), // address
    ];
    let test_call_storage_consistency_calldata = create_calldata(
        test_contract.get_instance_address(0),
        "test_call_storage_consistency",
        &test_call_storage_consistency_args,
    );
    txs.push(rpc_invoke_tx(
        account_tx_generator
            .build_invoke_tx_args()
            .calldata(test_call_storage_consistency_calldata),
    ));

    // 2. test_re_entrance - check calculations based on the storage of the newly deployed contract.
    let test_re_entrance_args = vec![
        *newly_deployed_contract_address.0.key(),
        felt!(2_u64), // depth
    ];
    let test_re_entrance_calldata = create_calldata(
        test_contract.get_instance_address(0),
        "test_re_entrance",
        &test_re_entrance_args,
    );
    txs.push(rpc_invoke_tx(
        account_tx_generator.build_invoke_tx_args().calldata(test_re_entrance_calldata),
    ));

    // 3. test_deploy_and_call - This function deploys yet another instance of test_contract.cairo
    //    and calls it.
    let deploy_and_call_args = vec![
        test_contract.get_class_hash().0, // class hash
        salt,                             // salt
        felt!(0_u64),                     // deploy_from_zero flag is down.
        felt!(2_u64),                     // constructor calldata length
        constructor_calldata_arg1,        // constructor arg1
        constructor_calldata_arg2,        // constructor arg2
        felt!(167_u64),                   // key for set_value
        felt!(160_u64),                   // value for set_value
    ];
    let deploy_and_call_calldata = create_calldata(
        test_contract.get_instance_address(0),
        "test_deploy_and_call",
        &deploy_and_call_args,
    );
    txs.push(rpc_invoke_tx(
        account_tx_generator.build_invoke_tx_args().calldata(deploy_and_call_calldata),
    ));

    // 4. set_value - Set value in storage by key via test_library_call of the deployed contract.
    let set_value_args = vec![
        felt!(2_u64),    // arguments length
        felt!(8_u64),    // key
        felt!(2023_u64), // value
    ];
    txs.push(account_tx_generator.generate_library_call_invoke_tx(
        &test_contract,
        &test_contract,
        "set_value",
        &set_value_args,
    ));

    txs
}
