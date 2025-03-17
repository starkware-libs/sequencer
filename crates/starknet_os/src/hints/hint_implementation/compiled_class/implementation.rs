use blockifier::state::state_api::StateReader;
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_relocatable_from_var_name,
};
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::types::relocatable::Relocatable;

use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::hint_implementation::compiled_class::utils::CompiledClassFact;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::{
    get_address_of_nested_fields,
    get_address_of_nested_fields_from_base_address,
    CairoSized,
    LoadCairoObject,
};

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
        &["hash"],
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

pub(crate) fn set_ap_to_segment_hash<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn validate_compiled_class_facts_post_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // TODO(Nimrod): Implement.
    Ok(())
}

// Hint extensions.
pub(crate) fn load_class_inner<S: StateReader>(
    HintArgs { hint_processor, constants, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    let identifier_getter = &hint_processor.execution_helper.os_program;
    let mut hint_extension = HintExtension::new();
    let mut compiled_class_facts_ptr = vm.add_memory_segment();
    // Iterate only over cairo 1 classes.
    for (class_hash, class) in hint_processor.execution_helper.os_input.compiled_classes.iter() {
        let compiled_class_fact = CompiledClassFact { class_hash, compiled_class: class };
        compiled_class_fact.load_into(
            vm,
            identifier_getter,
            compiled_class_facts_ptr,
            constants,
        )?;

        // Compiled classes are expected to end with a `ret` opcode followed by a pointer to
        // the builtin costs.
        let bytecode_ptr_address = get_address_of_nested_fields_from_base_address(
            compiled_class_facts_ptr,
            CairoStruct::CompiledClassFact,
            vm,
            &["compiled_class", "bytecode_ptr"],
            identifier_getter,
        )?;
        let bytecode_ptr = vm.get_relocatable(bytecode_ptr_address)?;
        let builtin_costs =
            get_relocatable_from_var_name(Ids::BuiltinCosts.into(), vm, ids_data, ap_tracking)?;
        let encoded_ret_opcode = 0x208b7fff7fff7ffe;
        let data = [encoded_ret_opcode.into(), builtin_costs.into()];
        vm.load_data((bytecode_ptr + class.bytecode.len())?, &data)?;

        // Extend hints.
        for (rel_pc, hints) in class.hints.iter() {
            let abs_pc = Relocatable::from((bytecode_ptr.segment_index, *rel_pc));
            hint_extension.insert(abs_pc, hints.iter().map(|h| any_box!(h.clone())).collect());
        }

        compiled_class_facts_ptr += CompiledClassFact::size(identifier_getter);
    }

    Ok(hint_extension)
}
