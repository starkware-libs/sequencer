use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;

use crate::test_utils::cairo_runner::EndpointArg;

#[derive(Debug, thiserror::Error)]
pub enum Cairo0EntryPointRunnerError {
    #[error(transparent)]
    CairoRun(#[from] CairoRunError),
    #[error(transparent)]
    ExplicitArg(#[from] ExplicitArgError),
    #[error(transparent)]
    VirtualMachine(#[from] VirtualMachineError),
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
