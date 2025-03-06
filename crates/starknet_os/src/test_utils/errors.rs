use cairo_vm::serde::deserialize_program::Member;
use cairo_vm::vm::errors::cairo_run_errors::CairoRunError;
use cairo_vm::vm::runners::cairo_runner::CairoArg;

#[derive(Debug, thiserror::Error)]
pub enum Cairo0EntryPointRunner {
    #[error(transparent)]
    CairoRun(#[from] CairoRunError),
    #[error(transparent)]
    ExplicitArg(#[from] ExplicitArg),
}

#[derive(Debug, thiserror::Error)]
pub enum ExplicitArg {
    #[error(
        "Expected arg {} to be {}, but actual type is {}. expected \
         arg: {:?}, actual arg: {:?}", info.index, info.expected_type, info.actual_type, 
         info.expected, info.actual
    )]
    Mismatch { info: Box<ArgMismatchInfo> },
    #[error(
        "Expected {} explicit arguments, got {}. Expected args: {expected:?}, actual args: \
        {actual:?}",
        .expected.len(), .actual.len()
    )]
    WrongNumberOfArgs { expected: Vec<Member>, actual: Vec<CairoArg> },
}

#[derive(Debug)]
pub struct ArgMismatchInfo {
    pub index: usize,
    pub expected_type: String,
    pub actual_type: String,
    pub expected: Member,
    pub actual: CairoArg,
}

impl From<ArgMismatchInfo> for ExplicitArg {
    fn from(info: ArgMismatchInfo) -> Self {
        Self::Mismatch { info: Box::new(info) }
    }
}
