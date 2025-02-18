use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ReplaceComponentError {
    #[error("Internal error.")]
    InternalError,
}
