use blockifier::state::state_api::StateReader;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn set_sha256_segment_in_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn log_remaining_txs<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn fill_holes_in_rc96_segment<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_component_hashes<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn sha2_finalize<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn segments_add_temp<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_ap_to_actual_fee<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn skip_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn start_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL at the new implementation of
    //   start_tx().
    todo!()
}

pub(crate) fn os_input_transactions<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn segments_add<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
