use blockifier::state::errors::StateError;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::vm::errors::hint_errors::HintError;
use starknet_types_core::felt::Felt;

use crate::hints::vars::{Const, Ids};

#[derive(Debug, thiserror::Error)]
pub enum OsHintError {
    #[error("{id:?} value {felt} is not a boolean.")]
    BooleanIdExpected { id: Ids, felt: Felt },
    #[error("Failed to convert felt value {felt:?} to const {variant:?} of type {ty}.")]
    ConstConversionError { variant: Const, felt: Felt, ty: String },
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    VmHintError(#[from] HintError),
    #[error("Unknown hint string: {0}")]
    UnknownHint(String),
}

/// `OsHintError` and the VM's `HintError` must have conversions in both directions, as execution
/// can pass back and forth between the VM and the OS hint processor; errors should propagate.
// TODO(Dori): Consider replicating the blockifier's mechanism and keeping structured error data,
//   instead of converting to string.
impl From<OsHintError> for HintError {
    fn from(error: OsHintError) -> Self {
        HintError::CustomHint(format!("{error}").into())
    }
}

pub type HintResult = Result<(), OsHintError>;
pub type HintExtensionResult = Result<HintExtension, OsHintError>;
