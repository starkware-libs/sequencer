use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ComponentError {
    #[error("Error in the component configuration.")]
    ComponentConfigError,
    #[error("An internal component error.")]
    InternalComponentError,
    #[error("Component has already been started.")]
    AlreadyStarted,
}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum ComponentServerError {
    #[error("Server has already been started.")]
    AlreadyStarted,
    #[error("Http server has failed: {0}.")]
    HttpServerStartError(String),
    #[error(transparent)]
    ComponentError(#[from] ComponentError),
    #[error("Server unexpectedly stopped.")]
    ServerUnexpectedlyStopped,
}
