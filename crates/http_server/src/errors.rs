use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpServerRunError {
    #[error(transparent)]
    ServerStartupError(#[from] hyper::Error),
}
