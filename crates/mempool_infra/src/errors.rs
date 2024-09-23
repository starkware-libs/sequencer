use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ComponentError {
    #[error("Error in the component configuration.")]
    ComponentConfigError,
    #[error("An internal component error.")]
    InternalComponentError,
}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ComponentServerError {
    #[error("Server has already been started.")]
    ServerAlreadyStarted,
    #[error("Http server has failed: {0}.")]
    HttpServerStartError(String),
    #[error(transparent)]
    ComponentError(#[from] ComponentError),
    #[error("Server unexpectedly stopped.")]
    ServerUnexpectedlyStopped,
}
