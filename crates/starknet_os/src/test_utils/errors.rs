use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;

#[derive(Debug, thiserror::Error)]
pub enum Cairo0EntryPointRunnerError {
    #[error("Invalid explicit argument: {error}")]
    InvalidExplicitArg { error: String },
    #[error("Expected {expected} explicit arguments, got {actual}.")]
    WrongNumberOfExplicitArgs { expected: usize, actual: usize },
    #[error(transparent)]
    CairoRunError(#[from] CairoRunError),
}
