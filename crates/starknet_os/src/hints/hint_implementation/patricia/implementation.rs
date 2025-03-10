use blockifier::state::state_api::StateReader;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::patricia::utils::DecodeNodeCase;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

pub(crate) fn set_siblings<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_case_right<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_bit<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_case_is_right<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    if case != DecodeNodeCase::Right {
        return Err(OsHintError::AssertionFailed { message: "case != 'right".to_string() });
    }
    Ok(())
}

pub(crate) fn write_case_not_left_to_ap<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn split_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn height_is_zero_or_len_node_preimage_is_two<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn prepare_preimage_validation_non_deterministic_hashes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn build_descent_map<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
