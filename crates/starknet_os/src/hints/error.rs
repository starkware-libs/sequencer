use blockifier::state::errors::StateError;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use starknet_types_core::felt::Felt;

use crate::hints::vars::{Const, Ids};

#[derive(Debug, thiserror::Error)]
pub enum OsHintError {
    #[error("Assertion failed: {0}")]
    AssertionFailed(String),
    #[error("{id:?} value {felt} is not a boolean.")]
    BooleanIdExpected { id: Ids, felt: Felt },
    #[error("Failed to convert {variant:?} felt value {felt:?} to type {ty}: {reason:?}.")]
    ConstConversionError { variant: Const, felt: Felt, ty: String, reason: String },
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    VmHintError(#[from] VmHintError),
    #[error("Unknown hint string: {0}")]
    UnknownHint(String),
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

pub type HintResult = Result<(), OsHintError>;
pub type HintExtensionResult = Result<HintExtension, OsHintError>;
