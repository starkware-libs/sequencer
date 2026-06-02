use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeederGatewayRunError {
    #[error(transparent)]
    ServerStartupError(#[from] io::Error),
}

/// Errors returned by feeder gateway request handling. The full client-facing error envelope
/// (HTTP status + legacy `StarknetError` body) is implemented in a later PR.
#[derive(Debug, Error)]
pub enum FeederGatewayError {
    // The source of an internal error is logged at the construction site and deliberately not
    // carried here, so nothing internal leaks to the client.
    #[error("Internal error")]
    Internal,
}
