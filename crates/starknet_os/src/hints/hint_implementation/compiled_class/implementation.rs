use std::collections::{BTreeMap, HashMap};
use std::vec::IntoIter;

use blockifier::state::state_api::StateReader;
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::types::relocatable::Relocatable;
use starknet_api::core::CompiledClassHash;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash as HashFunction;

use super::utils::BytecodeSegment;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::hint_implementation::compiled_class::utils::{
    create_bytecode_segment_structure,
    BytecodeSegmentNode,
    CompiledClassFact,
};
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{
    get_address_of_nested_fields,
    get_address_of_nested_fields_from_base_address,
    CairoSized,
    LoadCairoObject,
};

pub(crate) fn assign_bytecode_segments(mut ctx: HintContext<'_>) -> OsHintResult {
    let bytecode_segment_structure: BytecodeSegmentNode =
        ctx.get_from_scope(Scope::BytecodeSegmentStructure)?;

    match bytecode_segment_structure {
        BytecodeSegmentNode::InnerNode(node) => {
            ctx.insert_into_scope(Scope::BytecodeSegments, node.segments.into_iter());
            Ok(())
        }
        BytecodeSegmentNode::Leaf(_) => Err(OsHintError::AssignedLeafBytecodeSegment),
    }
}

pub(crate) fn assert_end_of_bytecode_segments(ctx: HintContext<'_>) -> OsHintResult {
    let bytecode_segments: &mut IntoIter<BytecodeSegment> =
        ctx.exec_scopes.get_mut_ref(Scope::BytecodeSegments.into())?;
    if bytecode_segments.next().is_some() {
        return Err(OsHintError::AssertionFailed {
            message: "The bytecode segments iterator is expected to be exhausted.".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn enter_scope_with_bytecode_segment_structure<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let bytecode_segment_structures: &BTreeMap<CompiledClassHash, BytecodeSegmentNode> =
        ctx.exec_scopes.get_ref(Scope::BytecodeSegmentStructures.into())?;

    let class_hash_address = get_address_of_nested_fields(
        ctx.ids_data,
        Ids::CompiledClassFact,
        CairoStruct::CompiledClassFactPtr,
        ctx.vm,
        ctx.ap_tracking,
        &["hash"],
        hint_processor.program,
    )?;
    let class_hash = CompiledClassHash(*ctx.vm.get_integer(class_hash_address)?.as_ref());
    let bytecode_segment_structure = bytecode_segment_structures
        .get(&class_hash)
        .ok_or_else(|| OsHintError::MissingBytecodeSegmentStructure(class_hash))?;

    // TODO(Nimrod): See if we can avoid the clone here.
    // We don't insert the `is_segment_used_callback` as a scope var as we use VM::is_accessed for
    // that.
    let new_scope = HashMap::from([(
        Scope::BytecodeSegmentStructure.into(),
        any_box!(bytecode_segment_structure.clone()),
    )]);
    ctx.exec_scopes.enter_scope(new_scope);

    Ok(())
}

pub(crate) fn delete_memory_data(ctx: HintContext<'_>) -> OsHintResult {
    let data_ptr = ctx.get_ptr(Ids::DataPtr)?;
    if ctx.vm.is_accessed(&data_ptr)? {
        return Err(OsHintError::AssertionFailed {
            message: format!("The segment {data_ptr} is skipped but was accessed."),
        });
    }
    ctx.vm.delete_unaccessed(data_ptr)?;
    Ok(())
}

pub(crate) fn is_leaf(mut ctx: HintContext<'_>) -> OsHintResult {
    let bytecode_segment_structure: &BytecodeSegmentNode =
        ctx.exec_scopes.get_ref(Scope::BytecodeSegmentStructure.into())?;
    let is_leaf = bytecode_segment_structure.is_leaf();
    Ok(ctx.insert_value(Ids::IsLeaf, Felt::from(is_leaf))?)
}

pub(crate) fn iter_current_segment_info(mut ctx: HintContext<'_>) -> OsHintResult {
    let bytecode_segments: &mut IntoIter<BytecodeSegment> =
        ctx.exec_scopes.get_mut_ref(Scope::BytecodeSegments.into())?;
    let current_segment_info = bytecode_segments
        .next()
        .ok_or(OsHintError::EndOfIterator { item_type: "Bytecode segments".to_string() })?;

    let data_ptr = ctx.get_ptr(Ids::DataPtr)?;

    let full_contract = ctx.get_integer(Ids::FullContract)?;

    let should_load = full_contract == Felt::ONE || ctx.vm.is_accessed(&data_ptr)?;

    if !should_load {
        for i in 0..current_segment_info.length() {
            let pc = (data_ptr + i)?;
            if ctx.vm.is_accessed(&pc)? {
                return Err(OsHintError::AssertionFailed {
                    message: format!(
                        "PC {} was visited, but the beginning of the segment ({}) was not",
                        pc.offset, data_ptr.offset
                    ),
                });
            }
        }
    }

    ctx.insert_value(Ids::LoadSegment, Felt::from(should_load))?;
    let is_leaf_and_loaded = should_load && current_segment_info.is_leaf();
    ctx.insert_value(Ids::IsLeafAndLoaded, Felt::from(is_leaf_and_loaded))?;
    ctx.insert_value(Ids::SegmentLength, current_segment_info.length())?;
    let new_scope = HashMap::from([(
        Scope::BytecodeSegmentStructure.into(),
        any_box!(current_segment_info.node),
    )]);
    ctx.exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn load_class<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    ctx.exec_scopes.exit_scope()?;
    let expected_hash_address = get_address_of_nested_fields(
        ctx.ids_data,
        Ids::CompiledClassFact,
        CairoStruct::CompiledClassFactPtr,
        ctx.vm,
        ctx.ap_tracking,
        &["hash"],
        hint_processor.program,
    )?;
    let expected_hash = ctx.vm.get_integer(expected_hash_address)?;
    let computed_hash = ctx.get_integer(Ids::Hash)?;
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

pub(crate) fn set_ap_to_segment_hash<H: HashFunction>(ctx: HintContext<'_>) -> OsHintResult {
    let bytecode_segment_structure: &BytecodeSegmentNode =
        ctx.exec_scopes.get_ref(Scope::BytecodeSegmentStructure.into())?;

    Ok(insert_value_into_ap(ctx.vm, bytecode_segment_structure.hash::<H>())?)
}

// Hint extensions.
pub(crate) fn load_classes_and_create_bytecode_segment_structures<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintExtensionResult {
    let identifier_getter = hint_processor.program;
    let mut hint_extension = HintExtension::new();
    let mut compiled_class_facts_ptr = ctx.vm.add_memory_segment();
    let mut bytecode_segment_structures = BTreeMap::new();
    // Insert n_compiled_class_facts, compiled_class_facts.
    ctx.insert_value(Ids::CompiledClassFacts, compiled_class_facts_ptr)?;
    ctx.insert_value(Ids::NCompiledClassFacts, hint_processor.compiled_classes.len())?;
    // Iterate only over cairo 1 classes.
    for (compiled_class_hash, compiled_class) in hint_processor.compiled_classes.iter() {
        let compiled_class_fact = CompiledClassFact { compiled_class_hash, compiled_class };
        compiled_class_fact.load_into(
            ctx.vm,
            identifier_getter,
            compiled_class_facts_ptr,
            ctx.constants,
        )?;

        // Compiled classes are expected to end with a `ret` opcode followed by a pointer to
        // the builtin costs.
        let bytecode_ptr_address = get_address_of_nested_fields_from_base_address(
            compiled_class_facts_ptr,
            CairoStruct::CompiledClassFact,
            ctx.vm,
            &["compiled_class", "bytecode_ptr"],
            identifier_getter,
        )?;
        let bytecode_ptr = ctx.vm.get_relocatable(bytecode_ptr_address)?;
        let builtin_costs = ctx.get_ptr(Ids::BuiltinCosts)?;
        let encoded_ret_opcode = 0x208b7fff7fff7ffe;
        let data = [encoded_ret_opcode.into(), builtin_costs.into()];
        ctx.vm.load_data((bytecode_ptr + compiled_class.bytecode.len())?, &data)?;

        // Extend hints.
        for (rel_pc, hints) in compiled_class.hints.iter() {
            let abs_pc = Relocatable::from((bytecode_ptr.segment_index, *rel_pc));
            hint_extension.insert(abs_pc, hints.iter().map(|h| any_box!(h.clone())).collect());
        }

        bytecode_segment_structures.insert(
            *compiled_class_hash,
            create_bytecode_segment_structure(
                &compiled_class.bytecode.iter().map(|x| Felt::from(&x.value)).collect::<Vec<_>>(),
                compiled_class.get_bytecode_segment_lengths(),
            )?,
        );

        compiled_class_facts_ptr += CompiledClassFact::size(identifier_getter)?;
    }

    // No need for is_segment_used callback: use the VM's `MemoryCell::is_accessed`.
    ctx.exec_scopes
        .insert_box(Scope::BytecodeSegmentStructures.into(), any_box!(bytecode_segment_structures));
    Ok(hint_extension)
}
