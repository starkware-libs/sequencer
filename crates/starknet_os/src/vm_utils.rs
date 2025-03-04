use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::{ApTracking, Identifier};
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::vars::{CairoStruct, Ids};

#[cfg(test)]
#[path = "vm_utils_test.rs"]
pub mod vm_utils_test;

pub(crate) trait IdentifierGetter {
    fn get_identifier(&self, identifier_name: &str) -> Result<&Identifier, OsHintError>;
}

impl IdentifierGetter for Program {
    fn get_identifier(&self, identifier_name: &str) -> Result<&Identifier, OsHintError> {
        Ok(self.get_identifier(identifier_name).ok_or_else(|| {
            HintError::UnknownIdentifier(identifier_name.to_string().into_boxed_str())
        })?)
    }
}

#[allow(dead_code)]
/// Fetches the address of nested fields of a cairo variable.
/// Example: Consider this hint: `ids.x.y.z`. This function fetches the address of `x`,
/// recursively fetches the offsets of `y` and `z`, and sums them up to get the address of `z`.
pub(crate) fn get_address_of_nested_fields<IG: IdentifierGetter>(
    ids_data: &HashMap<String, HintReference>,
    id: Ids,
    var_type: CairoStruct,
    vm: &VirtualMachine,
    ap_tracking: &ApTracking,
    nested_fields: &[String],
    identifier_getter: &IG,
) -> Result<Relocatable, OsHintError> {
    let base_address = get_ptr_from_var_name(id.into(), vm, ids_data, ap_tracking)?;
    get_address_of_nested_fields_from_base_address(
        base_address,
        var_type,
        vm,
        nested_fields,
        identifier_getter,
    )
}

/// Fetches the address of nested fields of a cairo variable, given a base address.
pub(crate) fn get_address_of_nested_fields_from_base_address<IG: IdentifierGetter>(
    base_address: Relocatable,
    var_type: CairoStruct,
    vm: &VirtualMachine,
    nested_fields: &[String],
    identifier_getter: &IG,
) -> Result<Relocatable, OsHintError> {
    let var_type_str = var_type.into();
    let base_struct = identifier_getter.get_identifier(var_type_str)?;

    fetch_nested_fields_address(base_address, base_struct, nested_fields, identifier_getter, vm)
}

/// Helper function to fetch the address of nested fields.
fn fetch_nested_fields_address<IG: IdentifierGetter>(
    base_address: Relocatable,
    base_struct: &Identifier,
    nested_fields: &[String],
    identifier_getter: &IG,
    vm: &VirtualMachine,
) -> Result<Relocatable, OsHintError> {
    let field = match nested_fields.first() {
        Some(first_field) => first_field,
        None => return Ok(base_address),
    };

    let base_struct_name = base_struct
        .full_name
        .as_ref()
        .ok_or_else(|| OsHintError::IdentifierHasNoFullName(Box::new(base_struct.clone())))?;

    let field_member = base_struct
        .members
        .as_ref()
        .ok_or_else(|| OsHintError::IdentifierHasNoMembers(Box::new(base_struct.clone())))?
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

    let new_base_struct = identifier_getter.get_identifier(cairo_type)?;

    fetch_nested_fields_address(
        new_base_address,
        new_base_struct,
        &nested_fields[1..],
        identifier_getter,
        vm,
    )
}

/// Inserts a value to a nested field of a cairo variable given a base address.
pub(crate) fn insert_value_to_nested_field<IG: IdentifierGetter, T: Into<MaybeRelocatable>>(
    base_address: Relocatable,
    var_type: CairoStruct,
    vm: &mut VirtualMachine,
    nested_fields: &[String],
    identifier_getter: &IG,
    val: T,
) -> OsHintResult {
    let nested_field_addr = get_address_of_nested_fields_from_base_address(
        base_address,
        var_type,
        vm,
        nested_fields,
        identifier_getter,
    )?;
    Ok(vm.insert_value(nested_field_addr, val)?)
}
