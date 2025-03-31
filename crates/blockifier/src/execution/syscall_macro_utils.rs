#[macro_export]
macro_rules! execute_syscall_macro {
    (SyscallHintProcessor, $self:ident, $vm:ident, $func_name:ident, $gas_cost_name:ident) => {
        $self.execute_syscall($vm, $func_name, $self.gas_costs().syscalls.$gas_cost_name)
    };
    (DeprecatedSyscallHintProcessor, $self:ident, $vm:ident, $func_name:ident) => {
        $self.execute_syscall($vm, $func_name)
    };
}

// TODO(Aner): enforce macro expansion correctness.
#[macro_export]
macro_rules! match_selector_to_execute_syscall {
    // TODO(Aner): use paste! macro to generate  function name and gas cost from variant name.
    (
        $self:ident,
        $vm:ident,
        $hint_processor_type:ident,
        $selector:ident,
        $enum_name:ident,
        $(($variant_name:ident, $func_name:ident$(, $gas_cost_name:ident)?)),+
    ) => {
        match $selector {
            $(
                $enum_name::$variant_name => $crate::execute_syscall_macro!(
                    $hint_processor_type,
                    $self,
                    $vm,
                    $func_name
                    $(, $gas_cost_name)?
                ),
            )+
            _ => Err(HintError::UnknownHint(
                format!("Unsupported syscall selector {:?}.", $selector).into(),
            )),
        }
    }
}
