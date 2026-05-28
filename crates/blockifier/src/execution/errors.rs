use std::collections::HashSet;

#[cfg(feature = "cairo_native")]
use cairo_native::error::Error as NativeError;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::runner_errors::RunnerError;
use cairo_vm::vm::errors::trace_errors::TraceError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use num_bigint::{BigInt, TryFromBigIntError};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use thiserror::Error;

use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::ConstructorContext;
use crate::execution::stack_trace::Cairo1RevertSummary;
#[cfg(feature = "cairo_native")]
use crate::execution::syscalls::hint_processor::SyscallExecutionError;
use crate::state::errors::StateError;
use crate::transaction::errors::NumericConversionError;

// TODO(AlonH, 21/12/2022): Implement Display for all types that appear in errors.

#[derive(Debug, Error)]
pub enum PreExecutionError {
    #[error("Entry point {:#066x} of type {typ:?} is not unique.", .selector.0)]
    DuplicatedEntryPointSelector { selector: EntryPointSelector, typ: EntryPointType },
    #[error("Entry point {0:?} not found in contract.")]
    EntryPointNotFound(EntryPointSelector),
    #[error("Fraud attempt blocked.")]
    FraudAttempt,
    #[error("Invalid builtin {0}.")]
    InvalidBuiltin(BuiltinName),
    #[error("The constructor entry point must be named 'constructor'.")]
    InvalidConstructorEntryPointName,
    #[error(transparent)]
    MathError(#[from] MathError),
    #[error(transparent)]
    MemoryError(#[from] MemoryError),
    #[error("No entry points of type {0:?} found in contract.")]
    NoEntryPointOfTypeFound(EntryPointType),
    #[error(transparent)]
    ProgramError(#[from] cairo_vm::types::errors::program_errors::ProgramError),
    #[error(transparent)]
    RunnerError(Box<RunnerError>),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error("Requested contract address {:#066x} is not deployed.", .0.key())]
    UninitializedStorageAddress(ContractAddress),
    #[error("Called builtins: {0:?} are unsupported in a Cairo0 contract")]
    UnsupportedCairo0Builtin(HashSet<BuiltinName>),
    #[error(
        "Insufficient entry point initial gas, must be greater than the entry point initial \
         budget."
    )]
    InsufficientEntryPointGas,
}

impl From<RunnerError> for PreExecutionError {
    fn from(error: RunnerError) -> Self {
        Self::RunnerError(Box::new(error))
    }
}

#[derive(Debug, Error)]
pub enum PostExecutionError {
    #[error(transparent)]
    MathError(#[from] MathError),
    #[error(transparent)]
    MemoryError(#[from] MemoryError),
    #[error(transparent)]
    RetdataSizeTooBig(#[from] TryFromBigIntError<BigInt>),
    #[error("Validation failed: {0}.")]
    SecurityValidationError(String),
    #[error(transparent)]
    VirtualMachineError(#[from] VirtualMachineError),
    #[error("Malformed return data : {error_message}.")]
    MalformedReturnData { error_message: String },
    #[error(transparent)]
    NumericConversionError(#[from] NumericConversionError),
}

impl From<RunnerError> for PostExecutionError {
    fn from(error: RunnerError) -> Self {
        Self::SecurityValidationError(error.to_string())
    }
}

#[derive(Debug, Error)]
pub enum EntryPointExecutionError {
    /// Tags an error with the executing contract's `TrackedResource` and the active
    /// `strip_vm_frames_in_sierra_gas` policy, for the stack-trace formatter to consume.
    /// Constructed only by `execute_entry_point_call_wrapper`.
    #[error("{inner}")]
    Annotated {
        #[source]
        inner: Box<EntryPointExecutionError>,
        tracked_resource: TrackedResource,
        strip_vm_frames_in_sierra_gas: bool,
    },
    #[error(transparent)]
    CairoRunError(#[from] Box<CairoRunError>),
    #[error("{error_trace}")]
    ExecutionFailed { error_trace: Cairo1RevertSummary },
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Invalid input: {input_descriptor}; {info}")]
    InvalidExecutionInput { input_descriptor: String, info: String },
    #[cfg(feature = "cairo_native")]
    #[error(transparent)]
    NativeUnexpectedError(#[from] NativeError),
    #[cfg(feature = "cairo_native")]
    #[error(transparent)]
    NativeUnrecoverableError(#[from] Box<SyscallExecutionError>),
    #[error(transparent)]
    PostExecutionError(#[from] PostExecutionError),
    #[error(transparent)]
    PreExecutionError(#[from] PreExecutionError),
    #[error("Execution failed due to recursion depth exceeded.")]
    RecursionDepthExceeded,
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    TraceError(#[from] TraceError),
}

impl EntryPointExecutionError {
    /// Tags `self` with the executing contract's `tracked_resource` and the active strip
    /// policy. No-op if already annotated — first wrap wins, so inner-frame info isn't
    /// overwritten as the error propagates through outer wrappers.
    pub fn annotated(
        self,
        tracked_resource: TrackedResource,
        strip_vm_frames_in_sierra_gas: bool,
    ) -> Self {
        match self {
            already @ EntryPointExecutionError::Annotated { .. } => already,
            other => EntryPointExecutionError::Annotated {
                inner: Box::new(other),
                tracked_resource,
                strip_vm_frames_in_sierra_gas,
            },
        }
    }

    /// Returns the deepest non-`Annotated` error — use at sites that pattern-match on a
    /// specific variant so the match isn't fooled by the wrapper's annotation.
    pub fn unannotated(&self) -> &Self {
        let mut current = self;
        while let EntryPointExecutionError::Annotated { inner, .. } = current {
            current = inner.as_ref();
        }
        current
    }

    /// Owned counterpart of `unannotated` — for pattern-matches that move subfields out.
    pub fn into_unannotated(mut self) -> Self {
        while let EntryPointExecutionError::Annotated { inner, .. } = self {
            self = *inner;
        }
        self
    }
}

#[derive(Debug, Error)]
pub enum ConstructorEntryPointExecutionError {
    #[error(
        "Error in the contract class {class_hash} constructor (selector: \
         {constructor_selector:?}, address: {contract_address:?}): {error}"
    )]
    ExecutionError {
        #[source]
        error: Box<EntryPointExecutionError>,
        class_hash: ClassHash,
        contract_address: ContractAddress,
        constructor_selector: Option<EntryPointSelector>,
    },
}

impl ConstructorEntryPointExecutionError {
    pub fn new(
        error: EntryPointExecutionError,
        ctor_context: &ConstructorContext,
        selector: Option<EntryPointSelector>,
    ) -> Self {
        Self::ExecutionError {
            error: Box::new(error),
            class_hash: ctor_context.class_hash,
            contract_address: ctor_context.storage_address,
            constructor_selector: selector,
        }
    }
}
