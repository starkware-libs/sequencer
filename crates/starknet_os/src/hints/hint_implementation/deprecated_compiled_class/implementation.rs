use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::{
    HintExtension,
    HintProcessorLogic,
    HintReference,
};
use cairo_vm::serde::deserialize_program::{HintParams, ReferenceManager};
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{get_address_of_nested_fields, LoadCairoObject};

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    insert_value_from_var_name(
        Ids::NCompiledClassFacts.into(),
        hint_processor.deprecated_class_hashes.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    exec_scopes.enter_scope(HashMap::new());
    Ok(())
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, constants }: HintArgs<'_>,
) -> OsHintResult {
    let (class_hash, deprecated_class) =
        hint_processor.deprecated_compiled_classes_iter.next().ok_or_else(|| {
            OsHintError::EndOfIterator { item_type: "deprecated_compiled_classes".to_string() }
        })?;

    let dep_class_base = vm.add_memory_segment();
    deprecated_class.load_into(vm, hint_processor.program, dep_class_base, constants)?;

    exec_scopes.insert_value(Scope::CompiledClassHash.into(), class_hash);
    exec_scopes.insert_value(Scope::CompiledClass.into(), deprecated_class);

    Ok(insert_value_from_var_name(
        Ids::CompiledClass.into(),
        dep_class_base,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn load_deprecated_class<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintExtensionResult {
    let computed_hash_addr = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClassFact,
        CairoStruct::DeprecatedCompiledClassFactPtr,
        vm,
        ap_tracking,
        &["hash"],
        hint_processor.program,
    )?;
    let computed_hash = vm.get_integer(computed_hash_addr)?;
    let expected_hash = exec_scopes.get::<ClassHash>(Scope::CompiledClassHash.into())?;

    if computed_hash.as_ref() != &expected_hash.0 {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Computed compiled_class_hash is inconsistent with the hash in the os_input. \
                 Computed hash = {computed_hash}, Expected hash = {expected_hash}."
            ),
        });
    }

    let dep_class = exec_scopes.get_ref::<ContractClass>(Scope::CompiledClass.into())?;

    let hints: HashMap<String, Vec<HintParams>> =
        serde_json::from_value(dep_class.program.hints.clone()).map_err(|e| {
            OsHintError::SerdeJsonDeserialize { error: e, value: dep_class.program.hints.clone() }
        })?;
    let ref_manager: ReferenceManager =
        serde_json::from_value(dep_class.program.reference_manager.clone()).map_err(|e| {
            OsHintError::SerdeJsonDeserialize {
                error: e,
                value: dep_class.program.reference_manager.clone(),
            }
        })?;

    let refs = ref_manager
        .references
        .iter()
        .map(|r| HintReference::from(r.clone()))
        .collect::<Vec<HintReference>>();

    let byte_code_ptr_addr = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClass,
        CairoStruct::DeprecatedCompiledClassPtr,
        vm,
        ap_tracking,
        &["bytecode_ptr"],
        hint_processor.program,
    )?;
    let byte_code_ptr = vm.get_relocatable(byte_code_ptr_addr)?;

    let mut hint_extension = HintExtension::new();

    for (pc, hints_params) in hints.into_iter() {
        let rel_pc = pc.parse().map_err(|_| VmHintError::WrongHintData)?;
        let abs_pc = Relocatable::from((byte_code_ptr.segment_index, rel_pc));
        let mut compiled_hints = Vec::new();
        for params in hints_params.into_iter() {
            let compiled_hint = hint_processor.compile_hint(
                &params.code,
                &params.flow_tracking_data.ap_tracking,
                &params.flow_tracking_data.reference_ids,
                &refs,
            )?;
            compiled_hints.push(compiled_hint);
        }
        hint_extension.insert(abs_pc, compiled_hints);
    }

    Ok(hint_extension)
}
