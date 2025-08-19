use blockifier::execution::deprecated_syscalls::deprecated_syscall_executor::execute_next_deprecated_syscall;
use blockifier::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallExecutionError;
use blockifier::execution::deprecated_syscalls::DeprecatedSyscallSelector;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;
use paste::paste;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};
use crate::syscall_handler_utils::SyscallHandlerType;

/// This macro generates syscall functions for the deprecated syscall hint processor.
/// Each function corresponds to a syscall and calls the `execute_next_deprecated_syscall`
/// function with the appropriate parameters.
/// Example usage:
///
/// create_syscall_func!(get_block_number);
///
/// Expands to:
///
/// pub(crate) fn get_block_number<S: StateReader>(
/// HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_>,
/// ) -> OsHintResult
/// {
///     assert_eq!(
///         exec_scopes.get(Scope::SyscallHandlerType.into())?,
///         SyscallHandlerType::DeprecatedSyscallHandler
///     );
///     let syscall_hint_processor = &mut hint_processor.deprecated_syscall_hint_processor;
///         let syscall_ptr = get_ptr_from_var_name(
///         Ids::SyscallPtr.into(), vm, ids_data, ap_tracking)?;
///         let syscall_selector = DeprecatedSyscallSelector::try_from(
///         vm.get_integer(syscall_ptr)?.into_owned()
///     )?;
///     let expected_selector = paste! { DeprecatedSyscallSelector::[$name:camel] };
///     if syscall_selector != expected_selector {
///         return Err(
///             DeprecatedSyscallExecutionError::BadSyscallSelector {
///                 expected_selector,
///                 actual_selector: syscall_selector,
///             }.into()
///         );
///     }
///     syscall_hint_processor.set_syscall_ptr(syscall_ptr);
///     Ok(
///         execute_next_deprecated_syscall(
///             syscall_hint_processor,
///             vm,
///             ids_data,
///             ap_tracking
///         )?
///     )
/// }
macro_rules! create_syscall_func {
    ($($name:ident),+) => {
        $(
            pub(crate) fn $name<S: StateReader>(
                hint_processor: & mut SnosHintProcessor<'_, S>,
                HintArgs {
                    vm,
                    ids_data,
                    ap_tracking,
                    exec_scopes,
                    ..
                }: HintArgs<'_>
            ) -> OsHintResult {
                assert_eq!(
                    exec_scopes.get::<SyscallHandlerType>(Scope::SyscallHandlerType.into())?,
                    SyscallHandlerType::DeprecatedSyscallHandler
                );
                let syscall_hint_processor = &mut hint_processor.get_mut_current_execution_helper()?.deprecated_syscall_hint_processor;
                let syscall_ptr = get_ptr_from_var_name(
                    Ids::SyscallPtr.into(), vm, ids_data, ap_tracking
                )?;
                let syscall_selector = DeprecatedSyscallSelector::try_from(
                    vm.get_integer(syscall_ptr)?.into_owned()
                )?;
                let expected_selector = paste! { DeprecatedSyscallSelector::[<$name:camel>] };
                if syscall_selector != expected_selector {
                    return Err(
                        Box::new(DeprecatedSyscallExecutionError::BadSyscallSelector {
                            expected_selector,
                            actual_selector: syscall_selector,
                        }).into()
                    );
                }
                syscall_hint_processor.set_syscall_ptr(syscall_ptr);
                Ok(
                    execute_next_deprecated_syscall(
                        hint_processor,
                        vm,
                        ids_data,
                        ap_tracking
                    )?
                )
            }
        )+
    };
}

create_syscall_func!(
    call_contract,
    delegate_call,
    delegate_l1_handler,
    deploy,
    emit_event,
    get_block_number,
    get_block_timestamp,
    get_caller_address,
    get_contract_address,
    get_sequencer_address,
    get_tx_info,
    get_tx_signature,
    library_call,
    library_call_l1_handler,
    replace_class,
    send_message_to_l1,
    storage_read,
    storage_write
);

pub(crate) fn set_syscall_ptr<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    assert!(
        !exec_scopes
            .get::<SyscallHandlerType>(Scope::SyscallHandlerType.into())
            .is_ok_and(|handler_type| handler_type == SyscallHandlerType::DeprecatedSyscallHandler),
        "Syscall handler type should either be unset or non-deprecated."
    );
    let syscall_ptr = get_ptr_from_var_name(Ids::SyscallPtr.into(), vm, ids_data, ap_tracking)?;
    hint_processor
        .get_mut_current_execution_helper()?
        .syscall_hint_processor
        .set_syscall_ptr(syscall_ptr);
    Ok(())
}
