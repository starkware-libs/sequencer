use thiserror::Error;

// #[derive(Error, Debug, PartialEq, Clone)]
// pub enum ComponentError {
//     #[error("Error in the component configuration.")]
//     ComponentConfigError,
//     #[error("An internal component error.")]
//     InternalComponentError,
// }

#[derive(Clone, Debug, Error)]
pub enum ReplaceComponentError {
    #[error("Internal error.")]
    InternalError,
}
