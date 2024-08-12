use cairo_lang_utils::byte_array::BYTE_ARRAY_MAGIC;
use starknet_types_core::felt::Felt;

use crate::execution::errors::EntryPointExecutionError;

#[test]
fn test_syscall_failure_format() {
    let error_data = vec![
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
    .collect();
    let error = EntryPointExecutionError::ExecutionFailed { error_data };
    assert_eq!(error.to_string(), "Execution failed. Failure reason: \"Execution failure\".");
}
