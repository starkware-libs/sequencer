use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::contract_class::EntryPointType;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{insert_value_to_nested_field, IdentifierGetter};

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
