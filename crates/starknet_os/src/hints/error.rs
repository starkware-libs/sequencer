use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::vm::errors::hint_errors::HintError;

#[derive(Debug, thiserror::Error)]
pub enum OsHintError {
    #[error(transparent)]
    VmHintError(#[from] HintError),
    #[error("Unknown hint string: {0}")]
    UnknownHint(String),
}

pub type HintResult = Result<(), HintError>;
pub type HintExtensionResult = Result<HintExtension, HintError>;
