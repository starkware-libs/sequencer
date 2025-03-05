use blockifier::execution::contract_class::EntryPointV1;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{insert_value_to_nested_field, IdentifierGetter};
#[allow(dead_code)]
/// Creates from `EntryPointV1` a cairo-0 `CompiledClassEntryPoint` struct and inserts it to the
/// VM at the given address.
fn insert_compiled_entry_point_class_to_vm<IG: IdentifierGetter>(
    vm: &mut VirtualMachine,
    entry_point: &EntryPointV1,
    identifier_getter: &IG,
    address: &Relocatable,
) -> OsHintResult {
    // Insert the fields.
    insert_value_to_nested_field(
        *address,
        CairoStruct::CompiledClassEntryPoint,
        vm,
        &["selector".to_string()],
        identifier_getter,
        entry_point.selector.0,
    )?;
    insert_value_to_nested_field(
        *address,
        CairoStruct::CompiledClassEntryPoint,
        vm,
        &["offset".to_string()],
        identifier_getter,
        entry_point.offset.0,
    )?;
    insert_value_to_nested_field(
        *address,
        CairoStruct::CompiledClassEntryPoint,
        vm,
        &["n_builtins".to_string()],
        identifier_getter,
        entry_point.builtins.len(),
    )?;

    // Allocate a segment for the builtin list.
    let builtin_list_base = &vm.add_memory_segment();

    // Cast the builtin names to felts.
    let builtins_data: Vec<MaybeRelocatable> = entry_point
        .builtins
        .iter()
        .map(|builtin| Felt::from_bytes_be_slice(builtin.to_str().as_bytes()).into())
        .collect();

    // Insert the builtin list.
    vm.load_data(*builtin_list_base, &builtins_data)?;

    // Insert the builtin list field.
    insert_value_to_nested_field(
        *address,
        CairoStruct::CompiledClassEntryPoint,
        vm,
        &["builtin_list".to_string()],
        identifier_getter,
        builtin_list_base,
    )?;

    Ok(())
}
