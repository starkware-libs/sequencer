use std::any::Any;
use std::collections::hash_map::IntoIter;
use std::collections::{HashMap, HashSet};

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
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{get_address_of_nested_fields, LoadCairoObject};

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let deprecated_compiled_classes = &hint_processor.deprecated_compiled_classes;
    // TODO(Rotem): see if we can avoid cloning here.
    let deprecated_class_hashes: HashSet<ClassHash> =
        HashSet::from_iter(deprecated_compiled_classes.keys().cloned());
    exec_scopes.insert_value(Scope::DeprecatedClassHashes.into(), deprecated_class_hashes);

    insert_value_from_var_name(
        Ids::NCompiledClassFacts.into(),
        deprecated_compiled_classes.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    // TODO(Nimrod): See if we can avoid cloning here.
    let scoped_classes: Box<dyn Any> = Box::new(deprecated_compiled_classes.clone().into_iter());
    exec_scopes
        .enter_scope(HashMap::from([(Scope::CompiledClassFacts.to_string(), scoped_classes)]));

    Ok(())
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, constants }: HintArgs<'_, S>,
) -> OsHintResult {
    let deprecated_class_iter = exec_scopes
        .get_mut_ref::<IntoIter<ClassHash, ContractClass>>(Scope::CompiledClassFacts.into())?;

    let (class_hash, deprecated_class) = deprecated_class_iter.next().ok_or_else(|| {
        OsHintError::EndOfIterator { item_type: "deprecated_compiled_classes".to_string() }
    })?;

    let dep_class_base = vm.add_memory_segment();
    deprecated_class.load_into(vm, &hint_processor.os_program, dep_class_base, constants)?;

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
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    let computed_hash_addr = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClassFact,
        CairoStruct::DeprecatedCompiledClassFact,
        vm,
        ap_tracking,
        &["hash"],
        &hint_processor.os_program,
    )?;
    let computed_hash = vm.get_integer(computed_hash_addr)?;
    let expected_hash = exec_scopes.get::<Felt>(Scope::CompiledClassHash.into())?;

    if computed_hash.as_ref() != &expected_hash {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Computed compiled_class_hash is inconsistent with the hash in the os_input. \
                 Computed hash = {computed_hash}, Expected hash = {expected_hash}."
            ),
        });
    }

    let dep_class = exec_scopes.get_ref::<ContractClass>(Scope::CompiledClass.into())?;

    // TODO(Rotem): see if we can avoid cloning here.
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
        CairoStruct::DeprecatedCompiledClass,
        vm,
        ap_tracking,
        &["bytecode_ptr"],
        &hint_processor.os_program,
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
