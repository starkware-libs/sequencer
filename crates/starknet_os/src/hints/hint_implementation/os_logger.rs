use blockifier::state::state_api::StateReader;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

pub(crate) fn os_logger_enter_syscall_prepare_exit_syscall<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.get_mut_current_execution_helper()?.os_logger.enter_syscall(
        selector,
        is_deprecated,
        n_steps,
        range_check_ptr,
        ids_data,
        vm,
        ap_tracking,
        os_program,
    )
}

pub(crate) fn os_logger_exit_syscall<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}
