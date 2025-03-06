use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;

#[derive(Debug, thiserror::Error)]
pub enum Cairo0EntryPointRunnerError {
    #[error("Invalid explicit argument: {error}")]
    InvalidExplicitArg { error: String },
    #[error("Invalid implicit argument: {error}")]
    InvalidImplicitArg { error: String },
    #[error("Expected {expected} explicit arguments, got {actual}.")]
    WrongNumberOfExplicitArgs { expected: usize, actual: usize },
    #[error("Expected {expected} implicit arguments, got {actual}.")]
    WrongNumberOfImplicitArgs { expected: usize, actual: usize },
    #[error(transparent)]
    CairoRunError(#[from] CairoRunError),
    #[error(transparent)]
    DeserializationError(#[from] serde_json::Error),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
}
