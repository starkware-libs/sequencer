use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

pub(crate) fn os_logger_enter_syscall_prepare_exit_syscall<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn os_logger_exit_syscall<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn log_enter_syscall<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}
