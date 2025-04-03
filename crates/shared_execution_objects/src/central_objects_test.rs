use blockifier::execution::call_info::{CallExecution, CallInfo};
use rstest::rstest;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::fields::Fee;

use crate::central_objects::CallInfoIndex;
use crate::central_objects::CentralTransactionExecutionInfo;

fn indexed_call_info(num: u64, inner_calls: Vec<CallInfo>) -> CallInfo {
    CallInfo {
        execution: CallExecution { gas_consumed: num, ..Default::default() },
        inner_calls,
        ..Default::default()
    }
}

fn get_execution_info() -> CentralTransactionExecutionInfo {
    CentralTransactionExecutionInfo {
        validate_call_info: Some(indexed_call_info(0, Vec::new())),
        execute_call_info: Some(indexed_call_info(
            1,
            Vec::from([
                indexed_call_info(2, Vec::from([indexed_call_info(3, Vec::new())])),
                indexed_call_info(4, Vec::new()),
            ]),
        )),
        fee_transfer_call_info: Some(indexed_call_info(5, Vec::new())),
        actual_fee: Fee(0),
        da_gas: GasVector::ZERO,
        actual_resources: Default::default(),
        revert_error: None,
        total_gas: GasVector::ZERO,
    }
}


#[rstest]
#[case::other_tx_type(TransactionType::InvokeFunction, vec![0, 1, 2, 3, 4, 5])]
#[case::deploy_account(TransactionType::DeployAccount, vec![1, 2, 3, 4, 0, 5])]
fn call_info_order_test(#[case] tx_type: TransactionType, #[case] expected_order: Vec<u64>) {
    let execution_info = get_execution_info();
    let ordered_nums = execution_info
        .call_info_iter(tx_type)
        .into_iter()
        .map(|call_info| call_info.execution.gas_consumed)
        .collect::<Vec<_>>();
    assert_eq!(ordered_nums, expected_order);
}

#[rstest]
#[case::other_tx_type(TransactionType::InvokeFunction, vec![0, 1, 2, 3, 4])]
#[case::deploy_account(TransactionType::DeployAccount, vec![1, 2, 3, 4, 0])]
fn tree_index_order_test(#[case] tx_type: TransactionType, #[case] expected_order: Vec<u64>) {
    let mut execution_info = get_execution_info();
    execution_info.fee_transfer_call_info = None;
    let mut call_info_index = CallInfoIndex::new(&execution_info, tx_type);
    let mut ordered_nums = Vec::new();

    loop {
        let current_call_info = call_info_index.current_call_info(&execution_info);
        if let Some(call_info) = current_call_info {
            ordered_nums.push(call_info.execution.gas_consumed);
            call_info_index.increment_call_info(call_info.inner_calls.len());
        } else {
            break;
        }
    }
    assert_eq!(ordered_nums, expected_order);
}
