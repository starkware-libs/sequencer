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
use starknet_api::transaction::fields::{Tip, TransactionSignature};

use crate::common::{end_to_end_flow, validate_tx_count, TestScenario};

mod common;

const DEFAULT_TIP: Tip = Tip(1_u64);
const CUSTOM_CAIRO_0_INVOKE_TX_COUNT: usize = 4;

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
/// The transactions are taken from starkware repo.
fn create_custom_cairo0_test_txs(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(CAIRO0_ACCOUNT_ID);
    let mut txs = vec![];
    txs.extend(generate_direct_test_contract_invoke_txs_cairo_0_syscall(account_tx_generator));
    txs.extend(generate_invoke_txs_with_signature_cairo_0_syscall(account_tx_generator));

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
        account_tx_generator
            .invoke_tx_builder()
            .tip(DEFAULT_TIP)
            .calldata(calldata)
            .build_rpc_invoke_tx()
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

    vec![
        account_tx_generator
            .invoke_tx_builder()
            .tip(DEFAULT_TIP)
            .calldata(calldata)
            .signature(signature)
            .build_rpc_invoke_tx(),
    ]
}
