use std::collections::HashSet;

use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;

use crate::test_utils::cairo_runner::{EndpointArg, ImplicitArg};

#[derive(Debug, thiserror::Error)]
pub enum Cairo0EntryPointRunnerError {
    #[error(transparent)]
    CairoRun(#[from] CairoRunError),
    #[error(transparent)]
    ExplicitArg(#[from] ExplicitArgError),
    #[error(transparent)]
    VirtualMachine(#[from] VirtualMachineError),
    #[error(transparent)]
    ImplicitArg(#[from] ImplicitArgError),
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error(transparent)]
    ProgramSerde(serde_json::Error),
    #[error(
        "The cairo runner's builtin list does not match the actual builtins, it should be \
         updated. Cairo Runner's builtins: {cairo_runner_builtins:?}, actual builtins: \
         {actual_builtins:?}"
    )]
    BuiltinMismatch {
        cairo_runner_builtins: Vec<BuiltinName>,
        actual_builtins: HashSet<BuiltinName>,
    },
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
