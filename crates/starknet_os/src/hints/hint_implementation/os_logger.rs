use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn os_logger_enter_syscall_prepare_exit_syscall(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}

pub(crate) fn os_logger_exit_syscall(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}
