use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::{ApTracking, Identifier};
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hints::error::OsHintError;
use crate::hints::vars::{CairoStruct, CairoStructPath};

#[cfg(test)]
#[path = "vm_utils_test.rs"]
pub mod vm_utils_test;

const PRIMITIVE_TYPES: [&str; 1] = ["felt"];
#[allow(dead_code)]
/// Fetches the address of nested fields of a cairo variable.
/// Example: Consider this hint: `ids.x.y.z`. This function fetches the address of `x`,
/// recursively fetches the offsets of `y` and `z`, and sums them up to get the address of `z`.
pub(crate) fn get_address_of_nested_fields(
    ids_data: &HashMap<String, HintReference>,
    var_name: &str,
    var_type: CairoStruct,
    vm: &VirtualMachine,
    ap_tracking: &ApTracking,
    nested_fields: &[String],
    identifiers: &HashMap<String, Identifier>,
) -> Result<Relocatable, OsHintError> {
    let base_address = get_ptr_from_var_name(var_name, vm, ids_data, ap_tracking)?;
    let type_path = CairoStructPath::from(var_type);
    let base_struct = identifiers
        .get(&type_path.0)
        .ok_or_else(|| HintError::UnknownIdentifier(type_path.0.into_boxed_str()))?;

    fetch_nested_fields_address(base_address, base_struct, nested_fields, identifiers, vm)
}

// Helper function to fetch the address of nested fields.
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

    let new_base_address = (base_address + field_member.offset).map_err(HintError::Math)?;

    match field_member.cairo_type.strip_suffix("*") {
        Some(actual_cairo_type) => {
            // If the field is a pointer, we remove the asterisk to know the exact type and
            // recursively fetch the address of the field.
            let new_base_address =
                vm.get_relocatable(new_base_address).map_err(HintError::Memory)?;

            if PRIMITIVE_TYPES.contains(&actual_cairo_type) {
                // Verify that a primitive is the last field in the chain.
                assert_eq!(nested_fields.len(), 1);
                // Return the address from the pointer.
                return Ok(new_base_address);
            }

            let new_base_struct = identifiers.get(actual_cairo_type).ok_or_else(|| {
                HintError::UnknownIdentifier(actual_cairo_type.to_string().into())
            })?;

            fetch_nested_fields_address(
                new_base_address,
                new_base_struct,
                &nested_fields[1..],
                identifiers,
                vm,
            )
        }
        None => {
            // If the field is not a pointer, we fetch the offset of the field.

            if PRIMITIVE_TYPES.contains(&field_member.cairo_type.as_str()) {
                // Verify that a primitive is the last field in the chain.
                assert_eq!(nested_fields.len(), 1);
                // Return the address from the pointer.
                return Ok(new_base_address);
            }
            let new_base_struct = identifiers.get(&field_member.cairo_type).ok_or_else(|| {
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
    }
}
