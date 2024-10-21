use thiserror::Error;

/// Errors originating from `[`AddTxEndpoint::run`]` command.
#[derive(Debug, Error)]
pub enum AddTxEndpointRunError {
    #[error(transparent)]
    ServerStartupError(#[from] hyper::Error),
}
