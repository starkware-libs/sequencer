use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::contract_class::EntryPointType;
use starknet_api::deprecated_contract_class::{ContractClass, EntryPointV0};
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::CairoStruct;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::vm_utils::{insert_values_to_fields ,insert_value_to_nested_field, IdentifierGetter, LoadCairoObject};

/// Returns the serialization of a contract as a list of field elements.
fn get_deprecated_contract_class_struct<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<S>,
    vm: &mut VirtualMachine,
    class_base: Relocatable,
    deprecated_class: ContractClass,
) -> OsHintResult {
    vm.insert_value(class_base, Felt::from(0))?; // DEPRECATED_COMPILED_CLASS_VERSION = 0

    let mut externals: Vec<MaybeRelocatable> = Vec::new();
    for elem in deprecated_class.entry_points_by_type.get(&EntryPointType::External).unwrap().iter() {
        externals.push(MaybeRelocatable::from(elem.selector.0));
        externals.push(MaybeRelocatable::from(Felt::from(elem.offset.0)));
    }
    vm.insert_value((class_base + 1)?, Felt::from(externals.len() / 2))?;
    let externals_base = vm.add_memory_segment();
    vm.load_data(externals_base, &externals)?;

    vm.insert_value((class_base + 2)?, externals_base)?;

    let mut l1_handlers: Vec<MaybeRelocatable> = Vec::new();
    for elem in deprecated_class.entry_points_by_type.get(&EntryPointType::L1Handler).unwrap().iter() {
        l1_handlers.push(MaybeRelocatable::from(elem.selector.0));
        l1_handlers.push(MaybeRelocatable::from(Felt::from(elem.offset.0)));
    }
    vm.insert_value((class_base + 3)?, Felt::from(l1_handlers.len() / 2))?;
    let l1_handlers_base = vm.add_memory_segment();
    vm.load_data(l1_handlers_base, &l1_handlers)?;

    vm.insert_value((class_base + 4)?, l1_handlers_base)?;

    let mut constructors: Vec<MaybeRelocatable> = Vec::new();
    for elem in deprecated_class.entry_points_by_type.get(&EntryPointType::Constructor).unwrap().iter() {
        constructors.push(MaybeRelocatable::from(elem.selector.0));
        constructors.push(MaybeRelocatable::from(Felt::from(elem.offset.0)));
    }
    vm.insert_value((class_base + 5)?, Felt::from(constructors.len() / 2))?;
    let constructors_base = vm.add_memory_segment();
    vm.load_data(constructors_base, &constructors)?;

    vm.insert_value((class_base + 6)?, constructors_base)?;

    let builtins: Vec<String> = serde_json::from_value(deprecated_class.clone().program.builtins).unwrap();
    let builtins: Vec<MaybeRelocatable> =
        builtins.into_iter().map(|bi| MaybeRelocatable::from(Felt::from_bytes_be_slice(bi.as_bytes()))).collect();

    vm.insert_value((class_base + 7)?, Felt::from(builtins.len()))?;
    let builtins_base = vm.add_memory_segment();
    vm.load_data(builtins_base, &builtins)?;
    vm.insert_value((class_base + 8)?, builtins_base)?;

    let contract_definition_dump = serde_json::to_vec(&deprecated_class).expect("Serialization should not fail");
    let mut cairo_contract_class_json =
        serde_json::from_slice::<json::CairoContractDefinition<'_>>(&contract_definition_dump)
            .expect("Deserialization should not fail");

    // This functions perform some tweaks for old Cairo contracts in order to keep backward compatibility and compute the right hash
    prepare_json_contract_definition(&mut cairo_contract_class_json)
        .map_err(|_| custom_hint_error("Processing Cairo contracts for backward compatibility failed"))?;

    let hinted_class_hash = {
        let class_hash =
            compute_cairo_hinted_class_hash(&cairo_contract_class_json).expect("Hashing should not fail here");
        Felt::from_bytes_be(&class_hash.to_be_bytes())
    };

    vm.insert_value((class_base + 9)?, hinted_class_hash)?;

    let data: Vec<String> = serde_json::from_value(deprecated_class.program.data).unwrap();
    let data: Vec<MaybeRelocatable> =
        data.into_iter().map(|datum| MaybeRelocatable::from(Felt::from_hex(&datum).unwrap())).collect();
    vm.insert_value((class_base + 10)?, Felt::from(data.len()))?;
    let data_base = vm.add_memory_segment();
    vm.load_data(data_base, &data)?;

    vm.insert_value((class_base + 11)?, data_base)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
/// Loads the entry points of a deprecated contract class to a contract class struct, given a
/// specific entry point type.
fn load_entry_points_to_contract_class_struct<IG: IdentifierGetter>(
    deprecated_class: &ContractClass,
    entry_point_type: &EntryPointType,
    class_base: Relocatable,
    var_type: CairoStruct,
    vm: &mut VirtualMachine,
    identifier_getter: &IG,
    entry_points_field: &str,
    num_entry_points_field: &str,
) -> OsHintResult {
    let empty_vec = Vec::new();
    let entry_points =
        deprecated_class.entry_points_by_type.get(entry_point_type).unwrap_or(&empty_vec);

    let flat_entry_point_data: Vec<MaybeRelocatable> = entry_points
        .iter()
        .flat_map(|entry_point| {
            vec![
                MaybeRelocatable::from(entry_point.selector.0),
                MaybeRelocatable::from(Felt::from(entry_point.offset.0)),
            ]
        })
        .collect();

    insert_value_to_nested_field(
        class_base,
        var_type,
        vm,
        &[num_entry_points_field],
        identifier_getter,
        Felt::from(entry_points.len()),
    )?;

    let flat_entry_point_data_base = vm.add_memory_segment();
    vm.load_data(flat_entry_point_data_base, &flat_entry_point_data)?;
    insert_value_to_nested_field(
        class_base,
        var_type,
        vm,
        &[entry_points_field],
        identifier_getter,
        flat_entry_point_data_base,
    )?;

    Ok(())
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
