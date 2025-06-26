use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::vm::errors::runner_errors::RunnerError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::errors::vm_exception::VmException;

use crate::io::os_input::OsInputError;

#[derive(Debug, thiserror::Error)]
pub enum StarknetOsError {
    #[error(transparent)]
    LoadProgramError(#[from] ProgramError),
    #[error(transparent)]
    OsInput(#[from] OsInputError),
    #[error(transparent)]
    RunnerError(#[from] RunnerError),
    #[error(transparent)]
    VmException(#[from] Box<VmException>),
    #[error(transparent)]
    VirtualMachineError(#[from] VirtualMachineError),
}
