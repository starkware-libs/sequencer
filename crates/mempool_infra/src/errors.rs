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
    #[error(transparent)]
    ComponentError(#[from] ComponentError),
    #[error("Http server has failed: {0}.")]
    AddTxEndpointStartError(String),
    #[error("Server unexpectedly stopped.")]
    ServerUnexpectedlyStopped,
}

#[derive(Clone, Debug, Error)]
pub enum ReplaceComponentError {
    #[error("Internal error.")]
    InternalError,
}
