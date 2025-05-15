use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
};
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::program::Program;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn os_logger_enter_syscall_prepare_exit_syscall<S: StateReader>(
    HintArgs { ap_tracking, ids_data, hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let is_deprecated = true;
    log_enter_syscall_helper(
        hint_processor,
        ids_data,
        ap_tracking,
        is_deprecated,
        hint_processor.os_program,
        vm,
    )
}

pub(crate) fn os_logger_exit_syscall<S: StateReader>(
    HintArgs { ap_tracking, ids_data, hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    let selector = get_integer_from_var_name(Ids::Selector.into(), vm, ids_data, ap_tracking)?;
    // TODO(Nimrod): Get `n_steps` by calling vm.get_current_step after we upgrade the VM.
    let dummy_n_steps = 7;
    let range_check_ptr =
        get_ptr_from_var_name(Ids::RangeCheckPtr.into(), vm, ids_data, ap_tracking)?;
    Ok(execution_helper.os_logger.exit_syscall(
        selector.try_into()?,
        dummy_n_steps,
        range_check_ptr,
        ids_data,
        vm,
        ap_tracking,
        hint_processor.os_program,
    )?)
}

pub(crate) fn log_enter_syscall<S: StateReader>(
    HintArgs { ap_tracking, ids_data, hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let is_deprecated = false;
    log_enter_syscall_helper(
        hint_processor,
        ids_data,
        ap_tracking,
        is_deprecated,
        hint_processor.os_program,
        vm,
    )
}

fn log_enter_syscall_helper<S: StateReader>(
    execution_helper: &mut SnosHintProcessor<'_, S>,
    ids_data: &HashMap<String, HintReference>,
    ap_tracking: &ApTracking,
    is_deprecated: bool,
    os_program: &Program,
    vm: &VirtualMachine,
) -> OsHintResult {
    let execution_helper = execution_helper.get_mut_current_execution_helper()?;
    // TODO(Nimrod): Get `n_steps` by calling vm.get_current_step after we upgrade the VM.
    let dummy_n_steps = 7;
    let range_check_ptr =
        get_ptr_from_var_name(Ids::RangeCheckPtr.into(), vm, ids_data, ap_tracking)?;
    let selector = get_integer_from_var_name(Ids::Selector.into(), vm, ids_data, ap_tracking)?;

    Ok(execution_helper.os_logger.enter_syscall(
        selector.try_into()?,
        is_deprecated,
        dummy_n_steps,
        range_check_ptr,
        ids_data,
        vm,
        ap_tracking,
        os_program,
    )?)
}
