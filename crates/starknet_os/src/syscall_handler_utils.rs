#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SyscallHandlerType {
    SyscallHandler,
    DeprecatedSyscallHandler,
}
