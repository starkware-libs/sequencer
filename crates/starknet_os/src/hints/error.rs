use blockifier::state::errors::StateError;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::serde::deserialize_program::Identifier;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::vm::errors::exec_scope_errors::ExecScopeError;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use starknet_api::block::BlockNumber;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;

use crate::hints::vars::{Const, Ids};

#[derive(Debug, thiserror::Error)]
pub enum OsHintError {
    #[error("Assertion failed: {message}")]
    AssertionFailed { message: String },
    #[error("Block number is probably < {stored_block_hash_buffer}.")]
    BlockNumberTooSmall { stored_block_hash_buffer: Felt },
    #[error("{id:?} value {felt} is not a boolean.")]
    BooleanIdExpected { id: Ids, felt: Felt },
    #[error("Failed to convert {variant:?} felt value {felt:?} to type {ty}: {reason:?}.")]
    ConstConversionError { variant: Const, felt: Felt, ty: String, reason: String },
    #[error(transparent)]
    ExecutionScopes(#[from] ExecScopeError),
    #[error("The identifier {0:?} has no full name.")]
    IdentifierHasNoFullName(Box<Identifier>),
    #[error("The identifier {0:?} has no members.")]
    IdentifierHasNoMembers(Box<Identifier>),
    #[error(
        "Inconsistent block numbers: {actual}, {expected}. The constant STORED_BLOCK_HASH_BUFFER \
         is probably out of sync."
    )]
    InconsistentBlockNumber { actual: BlockNumber, expected: BlockNumber },
    #[error(transparent)]
    MathError(#[from] MathError),
    #[error(transparent)]
    MemoryError(#[from] MemoryError),
    #[error("{error:?} for json value {value}.")]
    SerdeJsonError { error: serde_json::Error, value: serde_json::value::Value },
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error("Convert {n_bits} bits for {type_name}.")]
    StatelessCompressionOverflow { n_bits: usize, type_name: String },
    #[error("Unknown hint string: {0}")]
    UnknownHint(String),
    #[error(transparent)]
    VmError(#[from] VirtualMachineError),
    #[error(transparent)]
    VmHintError(#[from] VmHintError),
}

/// `OsHintError` and the VM's `HintError` must have conversions in both directions, as execution
/// can pass back and forth between the VM and the OS hint processor; errors should propagate.
// TODO(Dori): Consider replicating the blockifier's mechanism and keeping structured error data,
//   instead of converting to string.
impl From<OsHintError> for VmHintError {
    fn from(error: OsHintError) -> Self {
        Self::CustomHint(format!("{error}").into())
    }
}

pub type OsHintResult = Result<(), OsHintError>;
pub type OsHintExtensionResult = Result<HintExtension, OsHintError>;
