use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn call_contract<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn delegate_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn delegate_l1_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn deploy<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn emit_event<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn get_block_number<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_block_timestamp<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_caller_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_sequencer_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_tx_info<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn get_tx_signature<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn library_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn library_call_l1_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn replace_class<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn send_message_to_l1<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn storage_read<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn storage_write<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_syscall_ptr<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.syscall_hint_processor.set_syscall_ptr(get_ptr_from_var_name(
        Ids::SyscallPtr.into(),
        vm,
        ids_data,
        ap_tracking,
    )?);
    Ok(())
}
