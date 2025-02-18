use blockifier::state::state_api::StateReader;

use crate::hints::error::{HintExtensionResult, HintResult};
use crate::hints::types::HintArgs;

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn load_deprecated_class<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintExtensionResult {
    todo!()
}
