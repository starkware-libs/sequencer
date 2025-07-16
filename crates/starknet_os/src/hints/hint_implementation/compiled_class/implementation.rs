use std::collections::{BTreeMap, HashMap};
use std::vec::IntoIter;

use blockifier::state::state_api::StateReader;
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::types::relocatable::Relocatable;
use starknet_api::core::ClassHash;
use starknet_types_core::felt::Felt;

use super::utils::BytecodeSegment;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::hint_implementation::compiled_class::utils::{
    create_bytecode_segment_structure,
    BytecodeSegmentNode,
    CompiledClassFact,
};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{
    get_address_of_nested_fields,
    get_address_of_nested_fields_from_base_address,
    CairoSized,
    LoadCairoObject,
};

pub(crate) fn assign_bytecode_segments(HintArgs { exec_scopes, .. }: HintArgs<'_>) -> OsHintResult {
    let bytecode_segment_structure: BytecodeSegmentNode =
        exec_scopes.get(Scope::BytecodeSegmentStructure.into())?;

    match bytecode_segment_structure {
        BytecodeSegmentNode::InnerNode(node) => {
            exec_scopes.insert_value(Scope::BytecodeSegments.into(), node.segments.into_iter());
            Ok(())
        }
        BytecodeSegmentNode::Leaf(_) => Err(OsHintError::AssignedLeafBytecodeSegment),
    }
}

pub(crate) fn assert_end_of_bytecode_segments(
    HintArgs { exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    let bytecode_segments: &mut IntoIter<BytecodeSegment> =
        exec_scopes.get_mut_ref(Scope::BytecodeSegments.into())?;
    if bytecode_segments.next().is_some() {
        return Err(OsHintError::AssertionFailed {
            message: "The bytecode segments iterator is expected to be exhausted.".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn bytecode_segment_structure<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let bytecode_segment_structures: &BTreeMap<ClassHash, BytecodeSegmentNode> =
        exec_scopes.get_ref(Scope::BytecodeSegmentStructures.into())?;

    let class_hash_address = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClassFact,
        CairoStruct::CompiledClassFact,
        vm,
        ap_tracking,
        &["hash"],
        hint_processor.program,
    )?;

    let class_hash = ClassHash(*vm.get_integer(class_hash_address)?.as_ref());
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
    exec_scopes.enter_scope(new_scope);

    Ok(())
}

pub(crate) fn delete_memory_data(
    HintArgs { vm, ap_tracking, ids_data, .. }: HintArgs<'_>,
) -> OsHintResult {
    let data_ptr = get_ptr_from_var_name(Ids::DataPtr.into(), vm, ids_data, ap_tracking)?;
    if vm.is_accessed(&data_ptr)? {
        return Err(OsHintError::AssertionFailed {
            message: format!("The segment {data_ptr} is skipped but was accessed."),
        });
    }
    vm.delete_unaccessed(data_ptr)?;
    Ok(())
}

pub(crate) fn is_leaf(
    HintArgs { vm, exec_scopes, ap_tracking, ids_data, .. }: HintArgs<'_>,
) -> OsHintResult {
    let bytecode_segment_structure: &BytecodeSegmentNode =
        exec_scopes.get_ref(Scope::BytecodeSegmentStructure.into())?;
    let is_leaf = bytecode_segment_structure.is_leaf();
    Ok(insert_value_from_var_name(
        Ids::IsLeaf.into(),
        Felt::from(is_leaf),
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn iter_current_segment_info(
    HintArgs { exec_scopes, vm, ap_tracking, ids_data, .. }: HintArgs<'_>,
) -> OsHintResult {
    let bytecode_segments: &mut IntoIter<BytecodeSegment> =
        exec_scopes.get_mut_ref(Scope::BytecodeSegments.into())?;
    let current_segment_info = bytecode_segments
        .next()
        .ok_or(OsHintError::EndOfIterator { item_type: "Bytecode segments".to_string() })?;

    let data_ptr = get_ptr_from_var_name(Ids::DataPtr.into(), vm, ids_data, ap_tracking)?;

    #[cfg(test)]
    let is_used = {
        let leaf_always_accessed: bool =
            exec_scopes.get(Scope::LeafAlwaysAccessed.into()).unwrap_or(false);
        leaf_always_accessed || vm.is_accessed(&data_ptr)?
    };
    #[cfg(not(test))]
    let is_used = vm.is_accessed(&data_ptr)?;

    if !is_used {
        for i in 0..current_segment_info.length() {
            let pc = (data_ptr + i)?;
            if vm.is_accessed(&pc)? {
                return Err(OsHintError::AssertionFailed {
                    message: format!(
                        "PC {} was visited, but the beginning of the segment ({}) was not",
                        pc.offset, data_ptr.offset
                    ),
                });
            }
        }
    }

    insert_value_from_var_name(
        Ids::IsSegmentUsed.into(),
        Felt::from(is_used),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let is_used_leaf = is_used && current_segment_info.is_leaf();
    insert_value_from_var_name(
        Ids::IsUsedLeaf.into(),
        Felt::from(is_used_leaf),
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::SegmentLength.into(),
        current_segment_info.length(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let new_scope = HashMap::from([(
        Scope::BytecodeSegmentStructure.into(),
        any_box!(current_segment_info.node),
    )]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn load_class<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    exec_scopes.exit_scope()?;
    let expected_hash_address = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClassFact,
        CairoStruct::CompiledClassFact,
        vm,
        ap_tracking,
        &["hash"],
        hint_processor.program,
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

pub(crate) fn set_ap_to_segment_hash(
    HintArgs { exec_scopes, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let bytecode_segment_structure: &BytecodeSegmentNode =
        exec_scopes.get_ref(Scope::BytecodeSegmentStructure.into())?;

    Ok(insert_value_into_ap(vm, bytecode_segment_structure.hash().0)?)
}

pub(crate) fn validate_compiled_class_facts_post_execution<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { exec_scopes, .. }: HintArgs<'_>,
) -> OsHintResult {
    let mut bytecode_segment_structures = BTreeMap::new();
    for (compiled_hash, compiled_class) in hint_processor.compiled_classes.iter() {
        bytecode_segment_structures.insert(
            *compiled_hash,
            create_bytecode_segment_structure(
                &compiled_class.bytecode.iter().map(|x| Felt::from(&x.value)).collect::<Vec<_>>(),
                compiled_class.get_bytecode_segment_lengths(),
            )?,
        );
    }
    // No need for is_segment_used callback: use the VM's `MemoryCell::is_accessed`.
    exec_scopes.enter_scope(HashMap::from([(
        Scope::BytecodeSegmentStructures.into(),
        any_box!(bytecode_segment_structures),
    )]));
    Ok(())
}

// Hint extensions.
pub(crate) fn load_class_inner<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { constants, vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintExtensionResult {
    let identifier_getter = hint_processor.program;
    let mut hint_extension = HintExtension::new();
    let mut compiled_class_facts_ptr = vm.add_memory_segment();
    // Insert n_compiled_class_facts, compiled_class_facts.
    insert_value_from_var_name(
        Ids::CompiledClassFacts.into(),
        compiled_class_facts_ptr,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::NCompiledClassFacts.into(),
        hint_processor.compiled_classes.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    // Iterate only over cairo 1 classes.
    for (class_hash, class) in hint_processor.compiled_classes.iter() {
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
            get_ptr_from_var_name(Ids::BuiltinCosts.into(), vm, ids_data, ap_tracking)?;
        let encoded_ret_opcode = 0x208b7fff7fff7ffe;
        let data = [encoded_ret_opcode.into(), builtin_costs.into()];
        vm.load_data((bytecode_ptr + class.bytecode.len())?, &data)?;

        // Extend hints.
        for (rel_pc, hints) in class.hints.iter() {
            let abs_pc = Relocatable::from((bytecode_ptr.segment_index, *rel_pc));
            hint_extension.insert(abs_pc, hints.iter().map(|h| any_box!(h.clone())).collect());
        }

        compiled_class_facts_ptr += CompiledClassFact::size(identifier_getter)?;
    }

    Ok(hint_extension)
}
