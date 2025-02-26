use cairo_vm::types::errors::program_errors::ProgramError;

#[derive(Debug, thiserror::Error)]
pub enum StarknetOsError {
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
}
