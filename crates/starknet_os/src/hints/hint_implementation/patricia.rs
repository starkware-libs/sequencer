use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn set_siblings<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn is_case_right<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_bit<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_ap_to_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn assert_case_is_right<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn write_case_not_left_to_ap<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn split_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn height_is_zero_or_len_node_preimage_is_two<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn prepare_preimage_validation_non_deterministic_hashes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn build_descent_map<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
