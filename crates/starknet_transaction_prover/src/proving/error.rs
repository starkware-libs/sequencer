use thiserror::Error;

/// Errors from in-memory stwo proving.
#[derive(Debug, Error)]
pub enum StwoRunAndProveError {
    #[error("In-memory proving failed: {0}")]
    Proving(#[from] stwo_run_and_prove_lib::StwoRunAndProveError),

    #[error("Proving task failed to join: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}
