use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::{ApTracking, Identifier};
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hints::error::OsHintError;
use crate::hints::vars::{CairoStruct, Ids};

#[cfg(test)]
#[path = "vm_utils_test.rs"]
pub mod vm_utils_test;

#[allow(dead_code)]
/// Fetches the address of nested fields of a cairo variable.
/// Example: Consider this hint: `ids.x.y.z`. This function fetches the address of `x`,
/// recursively fetches the offsets of `y` and `z`, and sums them up to get the address of `z`.
pub(crate) fn get_address_of_nested_fields(
    ids_data: &HashMap<String, HintReference>,
    id: Ids,
    var_type: CairoStruct,
    vm: &VirtualMachine,
    ap_tracking: &ApTracking,
    nested_fields: &[String],
    identifiers: &HashMap<String, Identifier>,
) -> Result<Relocatable, OsHintError> {
    let base_address = get_ptr_from_var_name(id.into(), vm, ids_data, ap_tracking)?;
    let var_type_str = var_type.into();
    let base_struct = identifiers
        .get(var_type_str)
        .ok_or_else(|| HintError::UnknownIdentifier(var_type_str.to_string().into_boxed_str()))?;

    fetch_nested_fields_address(base_address, base_struct, nested_fields, identifiers, vm)
}

/// Helper function to fetch the address of nested fields.
fn fetch_nested_fields_address(
    base_address: Relocatable,
    base_struct: &Identifier,
    nested_fields: &[String],
    identifiers: &HashMap<String, Identifier>,
    vm: &VirtualMachine,
) -> Result<Relocatable, OsHintError> {
    let field = match nested_fields.first() {
        Some(first_field) => first_field,
        None => return Ok(base_address),
    };

    let base_struct_name = base_struct
        .full_name
        .as_ref()
        .ok_or_else(|| OsHintError::IdentifierHasNoFullName(format!("{:?}", base_struct)))?;

    let field_member = base_struct
        .members
        .as_ref()
        .ok_or_else(|| OsHintError::IdentifierHasNoMembers(format!("{:?}", base_struct)))?
        .get(field)
        .ok_or_else(|| {
            HintError::IdentifierHasNoMember(Box::from((
                base_struct_name.to_string(),
                field.to_string(),
            )))
        })?;

    let new_base_address = (base_address + field_member.offset)?;

    // If the field is a pointer, we remove the asterisk to know the exact type and
    // recursively fetch the address of the field.
    let (cairo_type, new_base_address) = match field_member.cairo_type.strip_suffix("*") {
        Some(actual_cairo_type) => (actual_cairo_type, vm.get_relocatable(new_base_address)?),
        None => (field_member.cairo_type.as_str(), new_base_address),
    };

    if nested_fields.len() == 1 {
        return Ok(new_base_address);
    }

    let new_base_struct = identifiers.get(cairo_type).ok_or_else(|| {
        HintError::UnknownIdentifier(field_member.cairo_type.clone().into_boxed_str())
    })?;

    fetch_nested_fields_address(
        new_base_address,
        new_base_struct,
        &nested_fields[1..],
        identifiers,
        vm,
    )
}
