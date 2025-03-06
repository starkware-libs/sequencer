use blockifier::execution::contract_class::{ContractClassV1Inner, EntryPointV1};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    insert_value_to_nested_field,
    CairoSized,
    IdentifierGetter,
    LoadCairoObject,
};

impl<IG: IdentifierGetter> LoadCairoObject<IG> for EntryPointV1 {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
    ) -> OsHintResult {
        let cairo_struct = CairoStruct::CompiledClassEntryPoint;
        // Insert the fields.
        insert_value_to_nested_field(
            address,
            cairo_struct,
            vm,
            &["selector".to_string()],
            identifier_getter,
            self.selector.0,
        )?;
        insert_value_to_nested_field(
            address,
            cairo_struct,
            vm,
            &["offset".to_string()],
            identifier_getter,
            self.offset.0,
        )?;
        insert_value_to_nested_field(
            address,
            cairo_struct,
            vm,
            &["n_builtins".to_string()],
            identifier_getter,
            self.builtins.len(),
        )?;

        // Allocate a segment for the builtin list.
        let builtin_list_base = vm.add_memory_segment();
        // Insert the builtin list.
        self.builtins.load_into(vm, identifier_getter, builtin_list_base)?;

        // Insert the builtin list field.
        insert_value_to_nested_field(
            address,
            cairo_struct,
            vm,
            &["builtin_list".to_string()],
            identifier_getter,
            builtin_list_base,
        )?;

        Ok(())
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for EntryPointV1 {
    fn size() -> usize {
        4
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for BuiltinName {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        _identifier_getter: &IG,
        address: Relocatable,
    ) -> OsHintResult {
        Ok(vm.insert_value(address, Felt::from_bytes_be_slice(self.to_str().as_bytes()))?)
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for BuiltinName {
    fn size() -> usize {
        1
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ContractClassV1Inner {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
    ) -> OsHintResult {
        // Insert compiled class version field.
        // TODO(Nimrod): Fetch from constants somehow.
        let version = Felt::from_bytes_be_slice("COMPILED_CLASS_V1".as_bytes());
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["compiled_class_version".to_string()],
            identifier_getter,
            version,
        )?;

        // Insert l1 handler entry points.
        let l1_handlers_list_base = vm.add_memory_segment();
        self.entry_points_by_type.l1_handler.load_into(
            vm,
            identifier_getter,
            l1_handlers_list_base,
        )?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["l1_handlers".to_string()],
            identifier_getter,
            l1_handlers_list_base,
        )?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["n_l1_handlers".to_string()],
            identifier_getter,
            self.entry_points_by_type.l1_handler.len(),
        )?;

        // Insert constructor entry points.
        let constructor_list_base = vm.add_memory_segment();
        self.entry_points_by_type.constructor.load_into(
            vm,
            identifier_getter,
            constructor_list_base,
        )?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["constructors".to_string()],
            identifier_getter,
            constructor_list_base,
        )?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["n_constructors".to_string()],
            identifier_getter,
            self.entry_points_by_type.constructor.len(),
        )?;

        // Insert external entry points.
        let externals_list_base = vm.add_memory_segment();
        self.entry_points_by_type.external.load_into(vm, identifier_getter, externals_list_base)?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["external_functions".to_string()],
            identifier_getter,
            externals_list_base,
        )?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["n_external_functions".to_string()],
            identifier_getter,
            self.entry_points_by_type.external.len(),
        )?;

        // Insert the bytecode entirely.
        let bytecode_base = vm.add_memory_segment();
        let copied_byte_code: Vec<_> = self.program.iter_data().cloned().collect();
        vm.load_data(bytecode_base, &copied_byte_code)?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["bytecode_ptr".to_string()],
            identifier_getter,
            bytecode_base,
        )?;
        insert_value_to_nested_field(
            address,
            CairoStruct::CompiledClass,
            vm,
            &["bytecode_length".to_string()],
            identifier_getter,
            copied_byte_code.len(),
        )?;

        Ok(())
    }
}
