use blockifier::execution::call_info::{CallExecution, CallInfo};
use rstest::rstest;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::fields::Fee;

use crate::central_objects::CentralTransactionExecutionInfo;

#[rstest]
#[case::deploy_account(TransactionType::InvokeFunction, vec![0, 1, 2, 3])]
#[case::other_tx_type(TransactionType::DeployAccount, vec![1, 2, 0, 3])]
fn call_info_order_test(#[case] tx_type: TransactionType, #[case] expected_order: Vec<u64>) {
    fn indexed_call_info(num: u64) -> CallInfo {
        CallInfo {
            execution: CallExecution { gas_consumed: num, ..Default::default() },
            ..Default::default()
        }
    }

    let validate_call_info = indexed_call_info(0);
    let mut execute_call_info = indexed_call_info(1);
    execute_call_info.inner_calls.push(indexed_call_info(2));
    let transfer_fee_call_info = indexed_call_info(3);
    let execution_info = CentralTransactionExecutionInfo {
        validate_call_info: Some(validate_call_info),
        execute_call_info: Some(execute_call_info),
        fee_transfer_call_info: Some(transfer_fee_call_info),
        actual_fee: Fee(0),
        da_gas: GasVector::ZERO,
        actual_resources: Default::default(),
        revert_error: None,
        total_gas: GasVector::ZERO,
    };
    let ordered_nums = execution_info
        .call_info_iter(tx_type)
        .map(|tx| tx.execution.gas_consumed)
        .collect::<Vec<_>>();
    assert_eq!(ordered_nums, expected_order);
}
