use blockifier::state::errors::StateError;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::serde::deserialize_program::Identifier;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;

use crate::hints::vars::{Const, Ids};

#[derive(Debug, thiserror::Error)]
pub enum OsHintError {
    #[error("Block number is probably < {stored_block_hash_buffer}.")]
    BlockNumberTooSmall { stored_block_hash_buffer: Felt },
    #[error("{id:?} value {felt} is not a boolean.")]
    BooleanIdExpected { id: Ids, felt: Felt },
    #[error("Failed to convert {variant:?} felt value {felt:?} to type {ty}: {reason:?}.")]
    ConstConversion { variant: Const, felt: Felt, ty: String, reason: String },
    #[error(
        "Inconsistent block numbers: {actual}, {expected}. The constant STORED_BLOCK_HASH_BUFFER \
         is probably out of sync."
    )]
    InconsistentBlockNumber { actual: BlockNumber, expected: BlockNumber },
    #[error(transparent)]
    Math(#[from] MathError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    VmHint(#[from] VmHintError),
    #[error("Unknown hint string: {0}")]
    UnknownHint(String),
    #[error("The identifier {0:?} has no full name.")]
    IdentifierHasNoFullName(Box<Identifier>),
    #[error("The identifier {0:?} has no members.")]
    IdentifierHasNoMembers(Box<Identifier>),
    #[error("Convert {n_bits} bits for {type_name}.")]
    StatelessCompressionOverflow { n_bits: usize, type_name: String },
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
