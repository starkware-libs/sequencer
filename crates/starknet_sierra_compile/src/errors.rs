use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use cairo_lang_starknet_classes::casm_contract_class::StarknetSierraCompilationError;
use thiserror::Error;

#[derive(Debug, Error)]
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
