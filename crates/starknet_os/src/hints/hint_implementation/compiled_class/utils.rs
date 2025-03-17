#![allow(dead_code)]
use std::collections::HashMap;

use cairo_lang_starknet_classes::casm_contract_class::{CasmContractClass, CasmContractEntryPoint};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::core::ClassHash;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::vars::{CairoStruct, Const};
use crate::vm_utils::{insert_values_to_fields, CairoSized, IdentifierGetter, LoadCairoObject};

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;

impl<IG: IdentifierGetter> LoadCairoObject<IG> for CasmContractEntryPoint {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        // Allocate a segment for the builtin list.
        let builtin_list_base = vm.add_memory_segment();
        let mut next_builtin_address = builtin_list_base;
        // Insert the builtin list.
        for builtin in self.builtins.iter() {
            let builtin_as_felt = Felt::from_bytes_be_slice(builtin.as_bytes());
            vm.insert_value(next_builtin_address, builtin_as_felt)?;
            next_builtin_address += 1;
        }
        // Insert the fields.
        let nested_fields_and_value = [
            ("selector".to_string(), Felt::from(&self.selector).into()),
            ("offset".to_string(), self.offset.into()),
            ("n_builtins".to_string(), self.builtins.len().into()),
            ("builtin_list".to_string(), builtin_list_base.into()),
        ];
        insert_values_to_fields(
            address,
            CairoStruct::CompiledClassEntryPoint,
            vm,
            nested_fields_and_value.as_slice(),
            identifier_getter,
        )?;

        Ok(())
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for CasmContractEntryPoint {
    fn size(_identifier_getter: &IG) -> usize {
        // TODO(Nimrod): Fetch from IG after we upgrade the VM.
        4
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for CasmContractClass {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        // Insert compiled class version field.
        let compiled_class_version = Const::CompiledClassVersion.fetch(constants)?;
        // Insert l1 handler entry points.
        let l1_handlers_list_base = vm.add_memory_segment();
        self.entry_points_by_type.l1_handler.load_into(
            vm,
            identifier_getter,
            l1_handlers_list_base,
            constants,
        )?;

        // Insert constructor entry points.
        let constructor_list_base = vm.add_memory_segment();
        self.entry_points_by_type.constructor.load_into(
            vm,
            identifier_getter,
            constructor_list_base,
            constants,
        )?;

        // Insert external entry points.
        let externals_list_base = vm.add_memory_segment();
        self.entry_points_by_type.external.load_into(
            vm,
            identifier_getter,
            externals_list_base,
            constants,
        )?;

        // Insert the bytecode entirely.
        let bytecode_base = vm.add_memory_segment();
        // TODO(Nimrod): See if we can transfer ownership here instead of cloning.

        let bytecode: Vec<_> =
            self.bytecode.iter().map(|x| MaybeRelocatable::from(Felt::from(&x.value))).collect();
        vm.load_data(bytecode_base, &bytecode)?;

        // Insert the fields.
        let nested_fields_and_value = [
            ("compiled_class_version".to_string(), compiled_class_version.into()),
            ("external_functions".to_string(), externals_list_base.into()),
            ("n_external_functions".to_string(), self.entry_points_by_type.external.len().into()),
            ("l1_handlers".to_string(), l1_handlers_list_base.into()),
            ("n_l1_handlers".to_string(), self.entry_points_by_type.l1_handler.len().into()),
            ("constructors".to_string(), constructor_list_base.into()),
            ("n_constructors".to_string(), self.entry_points_by_type.constructor.len().into()),
            ("bytecode_ptr".to_string(), bytecode_base.into()),
            ("bytecode_length".to_string(), bytecode.len().into()),
        ];

        insert_values_to_fields(
            address,
            CairoStruct::CompiledClass,
            vm,
            &nested_fields_and_value,
            identifier_getter,
        )?;

        Ok(())
    }
}

pub(crate) struct CompiledClassFact<'a> {
    pub(crate) class_hash: &'a ClassHash,
    pub(crate) compiled_class: &'a CasmContractClass,
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for CompiledClassFact<'_> {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        let compiled_class_address = vm.add_memory_segment();
        self.compiled_class.load_into(vm, identifier_getter, compiled_class_address, constants)?;
        let nested_fields_and_value = [
            ("class_hash".to_string(), self.class_hash.0.into()),
            ("compiled_class".to_string(), compiled_class_address.into()),
        ];
        insert_values_to_fields(
            address,
            CairoStruct::CompiledClassFact,
            vm,
            &nested_fields_and_value,
            identifier_getter,
        )
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for CompiledClassFact<'_> {
    fn size(_identifier_getter: &IG) -> usize {
        // TODO(Nimrod): Fetch from IG after we upgrade the VM.
        2
    }
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub(crate) struct BytecodeSegmentLeaf {
    pub(crate) data: Vec<Felt>,
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub(crate) struct BytecodeSegmentInnerNode {
    pub(crate) segments: Vec<BytecodeSegment>,
}

#[derive(Debug)]
pub(crate) enum BytecodeSegmentNode {
    Leaf(BytecodeSegmentLeaf),
    InnerNode(BytecodeSegmentInnerNode),
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub(crate) struct BytecodeSegment {
    node: BytecodeSegmentNode,
    length: usize,
}

/// Creates the bytecode segment structure from the given bytecode and bytecode segment lengths.
pub(crate) fn create_bytecode_segment_structure(
    bytecode: &[Felt],
    bytecode_segment_lengths: NestedIntList,
) -> Result<BytecodeSegmentNode, OsHintError> {
    let (structure, total_len) =
        create_bytecode_segment_structure_inner(bytecode, bytecode_segment_lengths, 0);
    // Sanity checks.
    if total_len != bytecode.len() {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Invalid length bytecode segment structure: {}. Bytecode length: {}.",
                total_len,
                bytecode.len()
            ),
        });
    }

    Ok(structure)
}

/// Helper function for `create_bytecode_segment_structure`.
/// Returns the bytecode segment structure and the total length of the processed segment.
pub(crate) fn create_bytecode_segment_structure_inner(
    bytecode: &[Felt],
    bytecode_segment_lengths: NestedIntList,
    bytecode_offset: usize,
) -> (BytecodeSegmentNode, usize) {
    match bytecode_segment_lengths {
        NestedIntList::Leaf(length) => {
            let segment_end = bytecode_offset + length;
            let bytecode_segment = bytecode[bytecode_offset..segment_end].to_vec();

            (BytecodeSegmentNode::Leaf(BytecodeSegmentLeaf { data: bytecode_segment }), length)
        }
        NestedIntList::Node(lengths) => {
            let mut segments = vec![];
            let mut total_len = 0;
            let mut bytecode_offset = bytecode_offset;

            for item in lengths {
                let (current_structure, item_len) =
                    create_bytecode_segment_structure_inner(bytecode, item, bytecode_offset);

                segments.push(BytecodeSegment { length: item_len, node: current_structure });

                bytecode_offset += item_len;
                total_len += item_len;
            }

            (BytecodeSegmentNode::InnerNode(BytecodeSegmentInnerNode { segments }), total_len)
        }
    }
}
