use std::collections::{BTreeMap, HashMap};

use blockifier::execution::contract_class::CompiledClassV0;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::hint_processor_definition::{HintExtension, HintProcessorLogic};
use cairo_vm::serde::deserialize_program::HintParams;
use cairo_vm::types::relocatable::Relocatable;
use starknet_api::core::ClassHash;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{get_address_of_nested_fields, LoadCairoObject};

pub(crate) fn load_deprecated_class_facts<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintArgs<'_>,
) -> OsHintResult {
    ctx.insert_value(Ids::NCompiledClassFacts, hint_processor.deprecated_class_hashes.len())?;
    ctx.exec_scopes.enter_scope(HashMap::new());
    Ok(())
}

pub(crate) fn load_deprecated_class_inner<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintArgs<'_>,
) -> OsHintResult {
    let (class_hash, deprecated_class) =
        hint_processor.deprecated_compiled_classes_iter.next().ok_or_else(|| {
            OsHintError::EndOfIterator { item_type: "deprecated_compiled_classes".to_string() }
        })?;

    let dep_class_base = ctx.vm.add_memory_segment();
    deprecated_class.load_into(ctx.vm, hint_processor.program, dep_class_base, ctx.constants)?;

    let compiled_class_v0 = CompiledClassV0::try_from(deprecated_class)?;

    ctx.exec_scopes.insert_value(Scope::ClassHash.into(), class_hash);
    ctx.exec_scopes.insert_value(Scope::CompiledClass.into(), compiled_class_v0);

    Ok(ctx.insert_value(Ids::CompiledClass, dep_class_base)?)
}

pub(crate) fn load_deprecated_class<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintArgs<'_>,
) -> OsHintExtensionResult {
    let computed_hash_addr = get_address_of_nested_fields(
        ctx.ids_data,
        Ids::CompiledClassFact,
        CairoStruct::DeprecatedCompiledClassFactPtr,
        ctx.vm,
        ctx.ap_tracking,
        &["hash"],
        hint_processor.program,
    )?;
    let computed_hash = ctx.vm.get_integer(computed_hash_addr)?;
    let expected_hash = ctx.exec_scopes.get::<ClassHash>(Scope::ClassHash.into())?;

    if computed_hash.as_ref() != &expected_hash.0 {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Computed compiled_class_hash is inconsistent with the hash in the os_input. \
                 Computed hash = {computed_hash}, Expected hash = {expected_hash}."
            ),
        });
    }

    let dep_class = ctx.exec_scopes.get_ref::<CompiledClassV0>(Scope::CompiledClass.into())?;

    let hints: BTreeMap<usize, Vec<HintParams>> =
        (&dep_class.program.shared_program_data.hints_collection).into();

    let byte_code_ptr_addr = get_address_of_nested_fields(
        ctx.ids_data,
        Ids::CompiledClass,
        CairoStruct::DeprecatedCompiledClassPtr,
        ctx.vm,
        ctx.ap_tracking,
        &["bytecode_ptr"],
        hint_processor.program,
    )?;
    let byte_code_ptr = ctx.vm.get_relocatable(byte_code_ptr_addr)?;
    let constants = dep_class.program.constants.clone();

    let mut hint_extension = HintExtension::new();

    for (pc, hints_params) in hints.iter() {
        let abs_pc = Relocatable::from((byte_code_ptr.segment_index, *pc));
        let mut compiled_hints = Vec::new();
        // TODO(Dori): handle accessible_scopes var.
        for params in hints_params.iter() {
            let compiled_hint = hint_processor.compile_hint(
                &params.code,
                &params.flow_tracking_data.ap_tracking,
                &params.flow_tracking_data.reference_ids,
                &dep_class.program.shared_program_data.reference_manager,
                &[],
                constants.clone(),
            )?;
            compiled_hints.push(compiled_hint);
        }
        hint_extension.insert(abs_pc, compiled_hints);
    }

    Ok(hint_extension)
}
