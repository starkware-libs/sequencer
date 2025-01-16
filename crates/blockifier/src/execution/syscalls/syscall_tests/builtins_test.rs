use rstest::rstest;
use rstest_reuse::apply;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::contract_class::SierraVersion;
use starknet_api::transaction::fields::Calldata;

use crate::context::{BlockContext, ChainInfo};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::test_templates::runnable_version;
use crate::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1, BALANCE};
use crate::transaction::objects::TransactionInfo;

#[apply(runnable_version)]
#[case::pedersen_vm("test_pedersen")]
#[case::bitwise_vm("test_bitwise")]
#[case::ecop_vm("test_ecop")]
#[case::poseidon_vm("test_poseidon")]
// This test case tests the add_mod and mul_mod builtins.
#[case::add_and_mul_mod_vm("test_circuit")]
fn builtins_test(runnable_version: RunnableCairo1, #[case] selector_name: &str) {
    use crate::context::TransactionContext;
    use crate::transaction::objects::CurrentTransactionInfo;

    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable_version));
    let chain_info = &ChainInfo::create_for_testing();
    let mut state = test_state(chain_info, BALANCE, &[(test_contract, 1)]);

    let calldata = Calldata(vec![].into());
    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name(selector_name),
        calldata,
        ..trivial_external_entry_point_new(test_contract)
    };

    let call_info_while_tracking_gas_consumed =
        entry_point_call.clone().execute_directly(&mut state).unwrap();

    let mut block_context = BlockContext::create_for_account_testing();
    block_context.versioned_constants.min_sierra_version_for_sierra_gas =
        SierraVersion::new(2, 8, 0);
    let tx_info = TransactionInfo::Current(CurrentTransactionInfo::create_for_testing());
    let tx_context = TransactionContext { block_context, tx_info };

    let call_info_while_tracking_vm_resources = entry_point_call
        .execute_directly_given_tx_context(&mut state, tx_context, false, ExecutionMode::Execute)
        .unwrap();

    println!("Gas consumed: {:?}", call_info_while_tracking_gas_consumed.execution.gas_consumed);
    println!("vm_resources: {:?}", call_info_while_tracking_vm_resources.resources);

    pretty_assertions::assert_eq!(
        call_info_while_tracking_vm_resources.execution,
        call_info_while_tracking_gas_consumed.execution
    );
    assert!(false);
}
