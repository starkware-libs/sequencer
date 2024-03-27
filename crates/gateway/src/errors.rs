use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error(transparent)]
    ConfigError(#[from] GatewayConfigError),
    #[error(transparent)]
    HTTPError(#[from] hyper::http::Error),
    #[error("Internal server error")]
    InternalServerError,
    #[error("Error while starting the server")]
    ServerStartError(#[from] hyper::Error),
}

#[derive(Debug, Error)]
pub enum GatewayConfigError {
    #[error("Server address is not an bind IP address: {0}")]
    InvalidServerBindAddress(String),
}
