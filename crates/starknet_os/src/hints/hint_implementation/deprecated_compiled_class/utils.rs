use std::collections::HashMap;

use cairo_vm::serde::deserialize_program::deserialize_array_of_bigint_hex;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::contract_class::EntryPointType;
use starknet_api::deprecated_contract_class::{ContractClass, EntryPointV0};
use starknet_types_core::felt::Felt;

use crate::hints::class_hash::hinted_class_hash::{
    compute_cairo_hinted_class_hash,
    CairoContractDefinition,
};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::vars::{CairoStruct, Const};
use crate::vm_utils::{insert_values_to_fields, CairoSized, IdentifierGetter, LoadCairoObject};

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ContractClass {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        // Insert compiled class version field.
        let compiled_class_version = Const::DeprecatedCompiledClassVersion.fetch(constants)?;

        // Insert external entry points.
        let (externals_list_base, externals_len) =
            insert_entry_points(self, vm, identifier_getter, constants, &EntryPointType::External)?;

        // Insert l1 handler entry points.
        let (l1_handlers_list_base, l1_handlers_len) = insert_entry_points(
            self,
            vm,
            identifier_getter,
            constants,
            &EntryPointType::L1Handler,
        )?;

        // Insert constructor entry points.
        let (constructors_list_base, constructors_len) = insert_entry_points(
            self,
            vm,
            identifier_getter,
            constants,
            &EntryPointType::Constructor,
        )?;

        // Insert builtins.
        let builtins: Vec<String> =
            serde_json::from_value(self.program.builtins.clone()).map_err(|e| {
                OsHintError::SerdeJsonDeserialize { error: e, value: self.program.builtins.clone() }
            })?;
        let builtins: Vec<MaybeRelocatable> = builtins
            .into_iter()
            .map(|bi| (Felt::from_bytes_be_slice(bi.as_bytes())).into())
            .collect();

        let builtin_list_base = vm.add_memory_segment();
        vm.load_data(builtin_list_base, &builtins)?;

        // Insert hinted class hash.
        let contract_definition_vec = serde_json::to_vec(&self)?;
        let contract_definition: CairoContractDefinition<'_> =
            serde_json::from_slice(&contract_definition_vec).map_err(OsHintError::SerdeJson)?;

        let hinted_class_hash = compute_cairo_hinted_class_hash(&contract_definition)?;

        // Insert bytecode_ptr.
        let bytecode_ptr = deserialize_array_of_bigint_hex(self.program.data.clone())?;

        let bytecode_ptr_base = vm.add_memory_segment();
        vm.load_data(bytecode_ptr_base, &bytecode_ptr)?;

        // Insert the fields.
        let nested_fields_and_value = [
            ("compiled_class_version", compiled_class_version.into()),
            ("n_external_functions", Felt::from(externals_len).into()),
            ("external_functions", externals_list_base.into()),
            ("n_l1_handlers", Felt::from(l1_handlers_len).into()),
            ("l1_handlers", l1_handlers_list_base.into()),
            ("n_constructors", Felt::from(constructors_len).into()),
            ("constructors", constructors_list_base.into()),
            ("n_builtins", Felt::from(builtins.len()).into()),
            ("builtin_list", builtin_list_base.into()),
            ("hinted_class_hash", hinted_class_hash.into()),
            ("bytecode_length", Felt::from(bytecode_ptr.len()).into()),
            ("bytecode_ptr", bytecode_ptr_base.into()),
        ];
        insert_values_to_fields(
            address,
            CairoStruct::DeprecatedCompiledClass,
            vm,
            nested_fields_and_value.as_slice(),
            identifier_getter,
        )?;

        Ok(())
    }
}

fn insert_entry_points<IG: IdentifierGetter>(
    dep_contract_class: &ContractClass,
    vm: &mut VirtualMachine,
    identifier_getter: &IG,
    constants: &HashMap<String, Felt>,
    entry_point_type: &EntryPointType,
) -> Result<(Relocatable, usize), OsHintError> {
    let empty_vec = Vec::new();
    let list_base = vm.add_memory_segment();
    let entry_points =
        dep_contract_class.entry_points_by_type.get(entry_point_type).unwrap_or(&empty_vec);
    entry_points.load_into(vm, identifier_getter, list_base, constants)?;
    Ok((list_base, entry_points.len()))
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for EntryPointV0 {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> OsHintResult {
        // Insert the fields.
        let nested_fields_and_value =
            [("selector", self.selector.0.into()), ("offset", self.offset.0.into())];
        insert_values_to_fields(
            address,
            CairoStruct::DeprecatedContractEntryPoint,
            vm,
            nested_fields_and_value.as_slice(),
            identifier_getter,
        )?;

        Ok(())
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for EntryPointV0 {
    fn size(_identifier_getter: &IG) -> usize {
        // TODO(Rotem): Fetch from IG after we upgrade the VM.
        2
    }
}
