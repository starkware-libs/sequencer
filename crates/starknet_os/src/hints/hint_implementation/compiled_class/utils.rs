use blockifier::execution::contract_class::EntryPointV1;
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
