use std::any::Any;
use std::collections::{HashMap, HashSet};

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{get_ptr_from_var_name, insert_value_from_var_name};
use starknet_api::core::ClassHash;

use crate::hints::error::{OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    // TODO(Rotem): see if we can avoid cloning here.
    let deprecated_class_hashes: HashSet<ClassHash> =
        HashSet::from_iter(os_input.deprecated_compiled_classes.keys().cloned());
    exec_scopes.insert_value(Scope::DeprecatedClassHashes.into(), deprecated_class_hashes);

    insert_value_from_var_name(
        Ids::NCompiledClassFacts.into(),
        os_input.deprecated_compiled_classes.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let scoped_classes: Box<dyn Any> =
        Box::new(os_input.deprecated_compiled_classes.clone().into_iter());
    exec_scopes
        .enter_scope(HashMap::from([(Scope::CompiledClassFacts.to_string(), scoped_classes)]));

    Ok(())
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_deprecated_class<S: StateReader>(
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintExtensionResult {
    let computed_hash_addr = get_ptr_from_var_name(vars::ids::COMPILED_CLASS_FACT, vm, ids_data, ap_tracking)?;
    let computed_hash = vm.get_integer(computed_hash_addr)?;
    let expected_hash = exec_scopes.get::<Felt252>(vars::scopes::COMPILED_CLASS_HASH).unwrap();

    if computed_hash.as_ref() != &expected_hash {
        return Err(HintError::AssertionFailed(
            format!(
                "Computed compiled_class_hash is inconsistent with the hash in the os_input. Computed hash = \
                 {computed_hash}, Expected hash = {expected_hash}."
            )
            .into_boxed_str(),
        ));
    }

    let dep_class = exec_scopes.get::<GenericDeprecatedCompiledClass>(vars::scopes::COMPILED_CLASS)?;
    let dep_class = dep_class.to_starknet_api_contract_class().map_err(|e| custom_hint_error(e.to_string()))?;

    let hints: HashMap<String, Vec<HintParams>> = serde_json::from_value(dep_class.program.hints).unwrap();
    let ref_manager: ReferenceManager = serde_json::from_value(dep_class.program.reference_manager).unwrap();
    let refs = ref_manager.references.iter().map(|r| HintReference::from(r.clone())).collect::<Vec<HintReference>>();

    let compiled_class_ptr = get_ptr_from_var_name(vars::ids::COMPILED_CLASS, vm, ids_data, ap_tracking)?;
    let byte_code_ptr = vm.get_relocatable((compiled_class_ptr + 11)?)?; //TODO: manage offset in a better way

    let mut hint_extension = HintExtension::new();

    for (pc, hints_params) in hints.into_iter() {
        let rel_pc = pc.parse().map_err(|_| HintError::WrongHintData)?;
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
