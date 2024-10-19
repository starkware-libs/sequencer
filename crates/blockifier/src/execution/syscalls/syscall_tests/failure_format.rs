use starknet_api::core::{ContractAddress, EntryPointSelector};
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::stack_trace::extract_trailing_cairo1_revert_trace;

#[test]
fn test_syscall_failure_format() {
    let execution_failure = "0x457865637574696f6e206661696c757265";
    let error_data = Retdata(vec![Felt::from_hex(execution_failure).unwrap()]);
    let callinfo = CallInfo {
        execution: CallExecution { retdata: error_data, failed: true, ..Default::default() },
        ..Default::default()
    };
    let error = EntryPointExecutionError::ExecutionFailed {
        error_trace: extract_trailing_cairo1_revert_trace(&callinfo),
    };
    assert_eq!(
        error.to_string(),
        format!(
            "Execution failed. Failure reason:
Error in contract (contract address: {:#064x}, selector: {:#064x}):
{execution_failure} ('Execution failure').",
            ContractAddress::default().0.key(),
            EntryPointSelector::default().0
        )
    );
}
