use std::collections::HashMap;

use blockifier::execution::contract_class::EntryPointV1;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    insert_values_to_nested_fields,
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
        constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        // Allocate a segment for the builtin list.
        let builtin_list_base = vm.add_memory_segment();
        // Insert the builtin list.
        self.builtins.load_into(vm, identifier_getter, builtin_list_base, constants)?;
        // Insert the fields.
        let selector_binding = ["selector".to_string()];
        let offset_binding = ["offset".to_string()];
        let n_builtins_binding = ["n_builtins".to_string()];
        let builtin_list_binding = ["builtin_list".to_string()];
        let nested_fields_and_value = [
            (selector_binding.as_slice(), self.selector.0.into()),
            (offset_binding.as_slice(), self.offset.0.into()),
            (n_builtins_binding.as_slice(), self.builtins.len().into()),
            (builtin_list_binding.as_slice(), builtin_list_base.into()),
        ];
        insert_values_to_nested_fields(
            address,
            CairoStruct::CompiledClassEntryPoint,
            vm,
            nested_fields_and_value.as_slice(),
            identifier_getter,
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
