use blockifier::execution::call_info::{CallExecution, CallInfo};
use rstest::rstest;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::constants::VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR;
use starknet_api::transaction::fields::Fee;

use crate::central_objects::CentralTransactionExecutionInfo;

fn call_info(num: u64, inner_calls: Vec<CallInfo>) -> CallInfo {
    CallInfo {
        execution: CallExecution { gas_consumed: num, ..Default::default() },
        inner_calls,
        ..Default::default()
    }
}

#[rstest]
#[case::other_tx_type(false, vec![0, 1, 2, 3, 4, 5])]
#[case::deploy_account(true, vec![1, 2, 3, 4, 0, 5])]
fn call_info_order_test(#[case] is_deploy_account: bool, #[case] expected_order: Vec<u64>) {
    let mut validate = call_info(0, vec![]);
    let execute =
        call_info(1, vec![call_info(2, vec![call_info(3, vec![])]), call_info(4, vec![])]);
    let fee_transfer = call_info(5, vec![]);

    if is_deploy_account {
        validate.call.entry_point_selector = *VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR;
    }

    let execution_info = CentralTransactionExecutionInfo {
        validate_call_info: Some(validate),
        execute_call_info: Some(execute),
        fee_transfer_call_info: Some(fee_transfer),
        actual_fee: Fee(0),
        da_gas: GasVector::ZERO,
        actual_resources: Default::default(),
        revert_error: None,
        total_gas: GasVector::ZERO,
    };
    let ordered: Vec<_> =
        execution_info.call_info_iter().map(|c| c.execution.gas_consumed).collect();
    assert_eq!(ordered, expected_order);
}
