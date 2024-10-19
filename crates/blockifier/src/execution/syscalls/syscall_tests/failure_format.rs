use cairo_lang_utils::byte_array::BYTE_ARRAY_MAGIC;
use starknet_api::core::{ContractAddress, EntryPointSelector};
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::stack_trace::extract_trailing_cairo1_revert_trace;

#[test]
fn test_syscall_failure_format() {
    let error_data = Retdata(
        vec![
            // Magic to indicate that this is a byte array.
            BYTE_ARRAY_MAGIC,
            // The number of full words in the byte array.
            "0x00",
            // The pending word of the byte array: "Execution failure"
            "0x457865637574696f6e206661696c757265",
            // The length of the pending word.
            "0x11",
        ]
        .into_iter()
        .map(|x| Felt::from_hex(x).unwrap())
        .collect(),
    );
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
Error in contract (contract address: {:#064x}, class hash: _, selector: {:#064x}):
\"Execution failure\".",
            ContractAddress::default().0.key(),
            EntryPointSelector::default().0
        )
    );
}
