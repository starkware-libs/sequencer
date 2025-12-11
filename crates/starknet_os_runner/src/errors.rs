use blockifier_reexecution::errors::ReexecutionError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionDataError {
    #[error(transparent)]
    ReexecutionError(#[from] ReexecutionError),
}
