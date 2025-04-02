use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use cairo_lang_starknet_classes::casm_contract_class::StarknetSierraCompilationError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompilationUtilError {
    #[error("Starknet Sierra compilation error: {0}")]
    CompilationError(String),
    #[error("Unexpected compilation error: {0}")]
    UnexpectedError(String),
}

impl From<AllowedLibfuncsError> for CompilationUtilError {
    fn from(error: AllowedLibfuncsError) -> Self {
        CompilationUtilError::CompilationError(error.to_string())
    }
}

impl From<StarknetSierraCompilationError> for CompilationUtilError {
    fn from(error: StarknetSierraCompilationError) -> Self {
        CompilationUtilError::CompilationError(error.to_string())
    }
}

impl From<serde_json::Error> for CompilationUtilError {
    fn from(error: serde_json::Error) -> Self {
        CompilationUtilError::UnexpectedError(error.to_string())
    }
}

impl From<std::io::Error> for CompilationUtilError {
    fn from(error: std::io::Error) -> Self {
        CompilationUtilError::UnexpectedError(error.to_string())
    }
}
