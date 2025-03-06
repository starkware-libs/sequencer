use std::collections::HashMap;

use blockifier::execution::contract_class::{ContractClassV1Inner, EntryPointV1};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::{CairoStruct, Const};
use crate::vm_utils::{insert_values_to_fields, CairoSized, IdentifierGetter, LoadCairoObject};

impl<IG: IdentifierGetter> LoadCairoObject<IG> for EntryPointV1 {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        // Allocate a segment for the builtin list.
        let builtin_list_base = vm.add_memory_segment();
        // Insert the builtin list.
        self.builtins.load_into(vm, identifier_getter, builtin_list_base, constants)?;
        // Insert the fields.
        let nested_fields_and_value = [
            ("selector".to_string(), self.selector.0.into()),
            ("offset".to_string(), self.offset.0.into()),
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

impl<IG: IdentifierGetter> CairoSized<IG> for EntryPointV1 {
    fn size(_identifier_getter: &IG) -> usize {
        // TODO(Nimrod): Fetch from IG after we upgrade the VM.
        4
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for BuiltinName {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        _identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        Ok(vm.insert_value(address, Felt::from_bytes_be_slice(self.to_str().as_bytes()))?)
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for BuiltinName {
    fn size(_identifier_getter: &IG) -> usize {
        // In cairo this is a felt.
        1
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ContractClassV1Inner {
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
        let copied_byte_code: Vec<_> = self.program.iter_data().cloned().collect();
        vm.load_data(bytecode_base, &copied_byte_code)?;

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
            ("bytecode_length".to_string(), copied_byte_code.len().into()),
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
