#[derive(Copy, Clone)]
pub(crate) enum SyscallHandlerType {
    SyscallHandler,
    DeprecatedSyscallHandler,
}
