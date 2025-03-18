use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::cairo_runner::CairoArg;

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
    Mismatch { index: usize, expected: Member, actual: CairoArg },
    #[error(
        "Expected {} explicit arguments, got {}. Expected args: {expected:?}, actual args: \
        {actual:?}",
        .expected.len(), .actual.len()
    )]
    WrongNumberOfArgs { expected: Vec<Member>, actual: Vec<CairoArg> },
    #[error(
        "Cairo runner does not support passing the endpoint structs / tuples / named tuples by \
         value. The endpoint can receive these types only via a pointer. unsupported param: \
         {expected_arg:?}, index: {index}"
    )]
    UnsupportedArgType { index: usize, expected_arg: Member },
    #[error(
        "To pass a pointer to the endpoint, use an array or a compound arg. index: {index}, bad \
         arg: {actual_arg:?}"
    )]
    SingleRelocatableParam { index: usize, actual_arg: CairoArg },
}
