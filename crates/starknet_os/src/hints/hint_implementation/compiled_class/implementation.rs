use blockifier::concurrency::test_utils::class_hash;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_integer_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;

use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::hint_implementation::compiled_class::utils::insert_compiled_class_to_vm;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::{get_address_of_nested_fields, insert_value_to_nested_field};

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

// Hint extensions.
pub(crate) fn load_class_inner<S: StateReader>(
    HintArgs { hint_processor, constants, vm, .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    let class_hash_to_contract_class =
        &hint_processor.execution_helper.cached_state.class_hash_to_class;
    let mut hint_extension = HintExtension::new();
    let mut compiled_class_facts = vm.add_memory_segment();
    // Iterate only over cairo 1 classes.
    for (class_hash, class) in
        class_hash_to_contract_class.borrow().iter().filter_map(|(class_hash, class)| {
            if let RunnableCompiledClass::V1(v1_class) = class {
                Some((class_hash, v1_class))
            } else {
                None
            }
        })
    {
        // Insert the class hash.
        insert_value_to_nested_field(
            compiled_class_facts,
            var_type,
            vm,
            nested_fields,
            identifier_getter,
            val,
        );
        // Allocate for the compiled class and fill.
        let compiled_class_address = vm.add_memory_segment();
        insert_compiled_class_to_vm(
            vm,
            class.0.as_ref(),
            &hint_processor.execution_helper.os_program,
            &compiled_class_address,
            constants,
        )?;
        // Insert the compiled class address.
        vm.insert_value(compiled_class_facts, compiled_class_address)?;
    }

    todo!()
}
