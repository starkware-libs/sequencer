use blockifier::state::state_api::StateReader;

use crate::hints::error::{OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;

pub(crate) fn assign_bytecode_segments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_end_of_bytecode_segments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn delete_memory_data<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(Yoni): Assert that the address was not accessed before.
    todo!()
}

pub(crate) fn iter_current_segment_info<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_class_inner<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_segment_hash<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn validate_compiled_class_facts_post_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

// Hint extension implementations.

pub(crate) fn load_class<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    todo!()
}
