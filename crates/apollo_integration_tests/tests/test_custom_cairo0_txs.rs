use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::ACCOUNT_ID_1 as CAIRO0_ACCOUNT_ID;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::felt;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::TransactionSignature;

use crate::common::{end_to_end_flow, validate_tx_count, TestScenario};

mod common;

const CUSTOM_CAIRO_0_INVOKE_TX_COUNT: usize = 5;

#[tokio::test]
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
    txs.extend(generate_deploy_contract_invoke_tx(account_tx_generator));

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

fn generate_deploy_contract_invoke_tx(
    account_tx_generator: &mut AccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let mut txs = vec![];
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let salt = felt!(34_u64);
    let constructor_calldata_arg1 = felt!(321_u64);
    let constructor_calldata_arg2 = felt!(543_u64);
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
    txs
}
