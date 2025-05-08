use std::collections::{BTreeMap, HashMap};

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

pub(crate) fn assign_bytecode_segments<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
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

pub(crate) fn assert_end_of_bytecode_segments<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn bytecode_segment_structure<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, ids_data, ap_tracking, vm, .. }: HintArgs<'_, '_, S>,
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
        hint_processor.os_program,
    )?;

    let class_hash = ClassHash(*vm.get_integer(class_hash_address)?.as_ref());
    let bytecode_segment_structure = bytecode_segment_structures
        .get(&class_hash)
        .ok_or_else(|| OsHintError::MissingBytecodeSegmentStructure(class_hash))?;

    // TODO(Nimrod): See if we can avoid the clone here.
    let new_scope = HashMap::from([(
        Scope::BytecodeSegmentStructure.into(),
        any_box!(bytecode_segment_structure.clone()),
    )]);
    // TODO(Nimrod): support is_segment_used_callback.
    exec_scopes.enter_scope(new_scope);

    Ok(())
}

pub(crate) fn delete_memory_data<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // TODO(Yoni): Assert that the address was not accessed before.
    todo!()
}

pub(crate) fn is_leaf<S: StateReader>(
    HintArgs { vm, exec_scopes, ap_tracking, ids_data, .. }: HintArgs<'_, '_, S>,
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

pub(crate) fn iter_current_segment_info<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_class<S: StateReader>(
    HintArgs { exec_scopes, ids_data, ap_tracking, vm, hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    exec_scopes.exit_scope()?;
    let expected_hash_address = get_address_of_nested_fields(
        ids_data,
        Ids::CompiledClassFact,
        CairoStruct::CompiledClassFact,
        vm,
        ap_tracking,
        &["hash"],
        hint_processor.os_program,
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
    HintArgs { exec_scopes, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let bytecode_segment_structure: &BytecodeSegmentNode =
        exec_scopes.get_ref(Scope::BytecodeSegmentStructure.into())?;

    Ok(insert_value_into_ap(vm, bytecode_segment_structure.hash().0)?)
}

pub(crate) fn validate_compiled_class_facts_post_execution<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, '_, S>,
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
    // TODO(Dori): upgrade the VM to a version including the `is_accessed` API, as added
    //   [here](https://github.com/lambdaclass/cairo-vm/pull/2024).
    exec_scopes.enter_scope(HashMap::from([(
        Scope::BytecodeSegmentStructures.into(),
        any_box!(bytecode_segment_structures),
    )]));
    Ok(())
}

// Hint extensions.
pub(crate) fn load_class_inner<S: StateReader>(
    HintArgs { hint_processor, constants, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintExtensionResult {
    let identifier_getter = hint_processor.os_program;
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

        compiled_class_facts_ptr += CompiledClassFact::size(identifier_getter);
    }

    Ok(hint_extension)
}
