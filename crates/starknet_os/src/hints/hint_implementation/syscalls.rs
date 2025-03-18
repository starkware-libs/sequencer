use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn call_contract<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn delegate_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn delegate_l1_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn deploy<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn emit_event<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn get_block_number<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn get_block_timestamp<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_caller_address<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_sequencer_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_tx_info<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn get_tx_signature<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn library_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn library_call_l1_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn replace_class<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn send_message_to_l1<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn storage_read<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn storage_write<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

// pub const SET_SYSCALL_PTR: &str = indoc! {r#"
// 	ids.os_context = segments.add()
// 	ids.syscall_ptr = segments.add()

// 	syscall_handler.set_syscall_ptr(syscall_ptr=ids.syscall_ptr)"#
// };

// pub fn set_syscall_ptr<PCS>(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError>
// where
//     PCS: PerContractStorage + 'static,
// {
//     let os_context = vm.add_memory_segment();
//     let syscall_ptr = vm.add_memory_segment();

//     insert_value_from_var_name(vars::ids::OS_CONTEXT, os_context, vm, ids_data, ap_tracking)?;
//     insert_value_from_var_name(vars::ids::SYSCALL_PTR, syscall_ptr, vm, ids_data, ap_tracking)?;

//     let syscall_handler: OsSyscallHandlerWrapper<PCS> =
// exec_scopes.get(vars::scopes::SYSCALL_HANDLER)?;     execute_coroutine(syscall_handler.
// set_syscall_ptr(syscall_ptr))?;

//     Ok(())
// }

pub(crate) fn set_syscall_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    hint_processor.syscall_hint_processor.set_syscall_ptr(get_ptr_from_var_name(
        Ids::SyscallPtr.into(),
        vm,
        ids_data,
        ap_tracking,
    )?);
    Ok(())
}
