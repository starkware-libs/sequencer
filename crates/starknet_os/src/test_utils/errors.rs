use std::collections::HashSet;

use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use strum::Display;

use crate::hints::error::OsHintError;
use crate::test_utils::cairo_runner::{EndpointArg, ImplicitArg};

#[derive(Debug, thiserror::Error)]
pub enum Cairo0EntryPointRunnerError {
    #[error(transparent)]
    ExplicitArg(#[from] ExplicitArgError),
    #[error(transparent)]
    VirtualMachine(#[from] VirtualMachineError),
    #[error(transparent)]
    ImplicitArg(#[from] ImplicitArgError),
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error(transparent)]
    ProgramSerde(#[from] serde_json::Error),
    #[error(transparent)]
    BuiltinMismatchError(#[from] BuiltinMismatchError),
    #[error(transparent)]
    RunCairoEndpoint(#[from] Box<CairoRunError>),
    #[error(transparent)]
    LoadReturnValue(#[from] LoadReturnValueError),
}

#[derive(Debug, thiserror::Error)]
pub enum ExplicitArgError {
    #[error(
        "Mismatch for explicit arg {index}. expected arg: {expected:?}, actual arg: {actual:?}"
    )]
    Mismatch { index: usize, expected: Member, actual: EndpointArg },
    #[error(
        "Expected {} explicit arguments, got {}. Expected args: {expected:?}, actual args: \
        {actual:?}",
        .expected.len(), .actual.len()
    )]
    WrongNumberOfArgs { expected: Vec<Member>, actual: Vec<EndpointArg> },
}

#[derive(Debug, thiserror::Error)]
pub enum ImplicitArgError {
    #[error(
        "Mismatch for implicit arg {index}. expected arg: {expected:?}, actual arg: {actual:?}"
    )]
    Mismatch { index: usize, expected: Member, actual: ImplicitArg },
    #[error(
        "Expected {} implicit arguments, got {}. Expected args: {expected:?}, actual args: \
        {actual:?}",
        .expected.len(), .actual.len()
    )]
    WrongNumberOfArgs { expected: Vec<(String, Member)>, actual: Vec<ImplicitArg> },
    #[error("Incorrect order of builtins. Expected: {correct_order:?}, actual: {actual_order:?}")]
    WrongBuiltinOrder { correct_order: Vec<BuiltinName>, actual_order: Vec<BuiltinName> },
}

#[derive(Debug, thiserror::Error)]
pub enum LoadReturnValueError {
    #[error(transparent)]
    Math(#[from] MathError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error(transparent)]
    BuiltinMismatchError(#[from] BuiltinMismatchError),
}

#[derive(Debug, thiserror::Error)]
#[error(
    "The cairo runner's builtin list does not match the actual builtins, it should be updated. \
     Cairo Runner's builtins: {cairo_runner_builtins:?}, actual builtins: {actual_builtins:?}"
)]
pub struct BuiltinMismatchError {
    pub cairo_runner_builtins: Vec<BuiltinName>,
    pub actual_builtins: HashSet<BuiltinName>,
}

#[derive(Debug, thiserror::Error, Display)]
pub enum OsSpecificTestError {
    Cairo0EntryPointRunner(#[from] Cairo0EntryPointRunnerError),
    OsHintError(#[from] OsHintError),
}
