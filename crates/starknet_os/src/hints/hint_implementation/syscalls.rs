use blockifier::execution::deprecated_syscalls::deprecated_syscall_executor::execute_next_deprecated_syscall;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;

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
/// HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
/// ) -> OsHintResult
/// {
///     let syscall_hint_processor = &mut hint_processor.deprecated_syscall_hint_processor;
///     Ok(execute_next_deprecated_syscall(syscall_hint_processor, vm, ids_data, ap_tracking)?)
/// }
macro_rules! create_syscall_func {
    ($($name:ident),+) => {
        $(
            pub(crate) fn $name<S: StateReader>(
                HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, '_, S>
            ) -> OsHintResult {
                assert_eq!(
                    exec_scopes.get::<SyscallHandlerType>(Scope::SyscallHandlerType.into())?,
                    SyscallHandlerType::DeprecatedSyscallHandler
                );
                let syscall_hint_processor = &mut hint_processor.deprecated_syscall_hint_processor;
                // TODO(Aner): need to verify that the correct syscall is being called (i.e.,
                //   syscall_ptr matches the fn name). E.g., set syscall_ptr from fn name.
                syscall_hint_processor.set_syscall_ptr(
                    get_ptr_from_var_name(Ids::SyscallPtr.into(), vm, ids_data, ap_tracking)?
                );
                Ok(
                    execute_next_deprecated_syscall(
                        syscall_hint_processor,
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
    HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_eq!(
        exec_scopes.get::<SyscallHandlerType>(Scope::SyscallHandlerType.into())?,
        SyscallHandlerType::SyscallHandler,
        "Called set_syscall_ptr in a deprecated syscall context."
    );
    let syscall_ptr = get_ptr_from_var_name(Ids::SyscallPtr.into(), vm, ids_data, ap_tracking)?;
    hint_processor.syscall_hint_processor.set_syscall_ptr(syscall_ptr);
    Ok(())
}
