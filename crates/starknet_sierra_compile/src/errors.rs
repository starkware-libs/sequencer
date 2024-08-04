use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use cairo_lang_starknet_classes::casm_contract_class::StarknetSierraCompilationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompilationUtilError {
    #[error(transparent)]
    AllowedLibfuncsError(#[from] AllowedLibfuncsError),
    #[error(transparent)]
    StarknetSierraCompilationError(#[from] StarknetSierraCompilationError),
    #[error("Compilation panicked")]
    CompilationPanic,
    #[error("Starknet Sierra compilation error: {0}")]
    CompilationError(String),
}
