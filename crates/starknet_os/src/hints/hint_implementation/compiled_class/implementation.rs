use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_integer_from_var_name;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn assign_bytecode_segments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_end_of_bytecode_segments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn delete_memory_data<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(Yoni): Assert that the address was not accessed before.
    todo!()
}

pub(crate) fn iter_current_segment_info<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_class<S: StateReader>(
    HintArgs { exec_scopes, ids_data, ap_tracking, vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    exec_scopes.exit_scope()?;
    let expected_hash_address = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClassFact,
        CairoStruct::CompiledClassFact,
        vm,
        ap_tracking,
        &["hash".to_string()],
        &hint_processor.execution_helper.os_program,
    )?;
    let expected_hash = vm.get_integer(expected_hash_address)?;
    let computed_hash = get_integer_from_var_name(Ids::Hash.into(), vm, ids_data, ap_tracking)?;
    if &computed_hash != expected_hash.as_ref() {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Computed compiled_class_hash is inconsistent with the hash in the os_input. \
                 Computed hash = {computed_hash}, Expected hash = {expected_hash}."
            ),
        });
    }

    Ok(())
}
pub(crate) fn load_class_inner<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_segment_hash<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn validate_compiled_class_facts_post_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}
