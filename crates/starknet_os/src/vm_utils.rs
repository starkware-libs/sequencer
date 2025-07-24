use std::collections::HashMap;

use apollo_starknet_os_program::CAIRO_FILES_MAP;
use blockifier::execution::execution_utils::ReadOnlySegment;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_relocatable_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::{ApTracking, Identifier, Location, Member};
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_exception::{get_error_attr_value, get_location};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;

use crate::hints::class_hash::hinted_class_hash::HintedClassHashError;
use crate::hints::vars::{CairoStruct, Ids};

#[cfg(test)]
#[path = "vm_utils_test.rs"]
pub mod vm_utils_test;

#[derive(Debug, thiserror::Error)]
pub enum VmUtilsError {
    #[error(transparent)]
    HintedClassHash(#[from] HintedClassHashError),
    #[error("The identifier {:?} has no member {} or it is of incorrect type", .0.0, .0.1)]
    IdentifierHasNoMember(Box<(Identifier, String)>),
    #[error("The identifier {0:?} has no members.")]
    IdentifierHasNoMembers(Box<Identifier>),
    #[error("The identifier {0:?} has no size.")]
    IdentifierHasNoSize(Box<Identifier>),
    #[error(transparent)]
    Math(#[from] MathError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error("Failed to parse resource bounds: {0}.")]
    ResourceBoundsParsing(StarknetApiError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error("{error:?} for json value {value}.")]
    SerdeJsonDeserialize { error: serde_json::Error, value: serde_json::value::Value },
    #[error(transparent)]
    VmHint(#[from] HintError),
}

pub type VmUtilsResult<T> = Result<T, VmUtilsError>;

pub(crate) trait LoadCairoObject<IG: IdentifierGetter> {
    /// Inserts the cairo 0 representation of `self` into the VM at the given address.
    /// Returns the next address after the inserted object.
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<Relocatable>;
}

pub(crate) trait CairoSized<IG: IdentifierGetter> {
    fn cairo_struct() -> CairoStruct;

    /// Returns the size of the cairo object.
    fn size(identifier_getter: &IG) -> VmUtilsResult<usize> {
        get_size_of_cairo_struct(Self::cairo_struct(), identifier_getter)
    }
}

pub(crate) trait IdentifierGetter {
    fn get_identifier(&self, identifier_name: &str) -> VmUtilsResult<&Identifier>;
}

impl IdentifierGetter for Program {
    fn get_identifier(&self, identifier_name: &str) -> VmUtilsResult<&Identifier> {
        Ok(self.get_identifier(identifier_name).ok_or_else(|| {
            HintError::UnknownIdentifier(identifier_name.to_string().into_boxed_str())
        })?)
    }
}

/// Fetches the address of nested fields of a cairo variable.
/// Example: Consider this hint: `ids.x.y.z`. This function fetches the address of `x`,
/// recursively fetches the offsets of `y` and `z`, and sums them up to get the address of `z`.
pub(crate) fn get_address_of_nested_fields<IG: IdentifierGetter>(
    ids_data: &HashMap<String, HintReference>,
    id: Ids,
    var_type: CairoStruct,
    vm: &VirtualMachine,
    ap_tracking: &ApTracking,
    nested_fields: &[&str],
    identifier_getter: &IG,
) -> VmUtilsResult<Relocatable> {
    let base_address = get_relocatable_from_var_name(id.into(), vm, ids_data, ap_tracking)?;

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
    nested_fields: &[&str],
    identifier_getter: &IG,
) -> VmUtilsResult<Relocatable> {
    let (actual_type, actual_base_address) =
        deref_type_and_address_if_ptr(var_type.into(), base_address, vm)?;
    let base_struct = identifier_getter.get_identifier(actual_type)?;

    fetch_nested_fields_address(
        actual_base_address,
        base_struct,
        nested_fields,
        identifier_getter,
        vm,
    )
}

/// Returns the actual type and the actual address of variable or a field, depending on whether or
/// not the type is a pointer.
fn deref_type_and_address_if_ptr<'a>(
    cairo_type: &'a str,
    base_address: Relocatable,
    vm: &VirtualMachine,
) -> Result<(&'a str, Relocatable), VmUtilsError> {
    Ok(match cairo_type.strip_suffix("*") {
        Some(actual_cairo_type) => (actual_cairo_type, vm.get_relocatable(base_address)?),
        None => (cairo_type, base_address),
    })
}

fn fetch_field_member<'a>(base_struct: &'a Identifier, field: &str) -> VmUtilsResult<&'a Member> {
    base_struct
        .members
        .as_ref()
        .ok_or_else(|| VmUtilsError::IdentifierHasNoMembers(Box::new(base_struct.clone())))?
        .get(&field.to_string())
        .ok_or_else(|| {
            VmUtilsError::IdentifierHasNoMember(Box::from((base_struct.clone(), field.to_string())))
        })
}

/// Helper function to fetch the address of nested fields.
/// Implicitly dereferences the type if it is a pointer.
/// For the last field, it returns the address of the field.
fn fetch_nested_fields_address<IG: IdentifierGetter>(
    base_address: Relocatable,
    base_struct: &Identifier,
    nested_fields: &[&str],
    identifier_getter: &IG,
    vm: &VirtualMachine,
) -> VmUtilsResult<Relocatable> {
    let field = match nested_fields.first() {
        Some(first_field) => first_field,
        None => return Ok(base_address),
    };

    let field_member = fetch_field_member(base_struct, field)?;

    let address_with_offset = (base_address + field_member.offset)?;

    // If this is the innermost field, do not dereference the address even if it is a pointer.
    // Otherwise, check if the field is a pointer type, and if so, dereference.
    if nested_fields.len() == 1 {
        return Ok(address_with_offset);
    }

    let (cairo_type, new_base_address) =
        deref_type_and_address_if_ptr(&field_member.cairo_type, address_with_offset, vm)?;

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
    nested_fields: &[&str],
    identifier_getter: &IG,
    val: T,
) -> VmUtilsResult<()> {
    let nested_field_addr = get_address_of_nested_fields_from_base_address(
        base_address,
        var_type,
        vm,
        nested_fields,
        identifier_getter,
    )?;
    Ok(vm.insert_value(nested_field_addr, val)?)
}

/// Inserts each value to a field of a cairo variable given a base address.
pub(crate) fn insert_values_to_fields<IG: IdentifierGetter>(
    base_address: Relocatable,
    var_type: CairoStruct,
    vm: &mut VirtualMachine,
    nested_fields_and_value: &[(&str, MaybeRelocatable)],
    identifier_getter: &IG,
) -> VmUtilsResult<()> {
    for (nested_fields, value) in nested_fields_and_value {
        insert_value_to_nested_field(
            base_address,
            var_type,
            vm,
            &[nested_fields],
            identifier_getter,
            value,
        )?;
    }
    Ok(())
}

impl<IG: IdentifierGetter, T: LoadCairoObject<IG>> LoadCairoObject<IG> for Vec<T> {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<Relocatable> {
        let mut next_address = address;
        for t in self.iter() {
            next_address = t.load_into(vm, identifier_getter, next_address, constants)?;
        }
        Ok(next_address)
    }
}

/// Returns the offset of a field in a cairo struct.
pub(crate) fn get_field_offset<IG: IdentifierGetter>(
    var_type: CairoStruct,
    field: &str,
    identifier_getter: &IG,
) -> VmUtilsResult<usize> {
    let base_struct = identifier_getter.get_identifier(var_type.into())?;
    let field_member = fetch_field_member(base_struct, field)?;
    Ok(field_member.offset)
}

/// Returns the size of a cairo struct. If it's a pointer, it returns 1.
pub(crate) fn get_size_of_cairo_struct<IG: IdentifierGetter>(
    cairo_type: CairoStruct,
    identifier_getter: &IG,
) -> VmUtilsResult<usize> {
    let cairo_type_str: &str = cairo_type.into();

    if cairo_type_str.ends_with("*") {
        return Ok(1);
    }

    let base_struct = identifier_getter.get_identifier(cairo_type_str)?;

    base_struct.size.ok_or_else(|| VmUtilsError::IdentifierHasNoSize(Box::new(base_struct.clone())))
}

/// Write the given data to a temporary segment in the VM. Returns the written data segment.
pub(crate) fn write_to_temp_segment(
    data: &[Felt],
    vm: &mut VirtualMachine,
) -> Result<ReadOnlySegment, MemoryError> {
    let relocatable_data: Vec<_> = data.iter().map(MaybeRelocatable::from).collect();
    let segment_start_ptr = vm.add_temporary_segment();
    vm.load_data(segment_start_ptr, &relocatable_data)?;
    Ok(ReadOnlySegment { start_ptr: segment_start_ptr, length: relocatable_data.len() })
}

/// Adds code snippets to the traceback of a VM exception.
#[allow(dead_code)]
pub(crate) fn get_traceback_with_code_snippet(runner: &CairoRunner) -> Option<String> {
    // It's almost similar to `get_traceback` in `cairo_vm::vm::errors::vm_exception`, but we add
    // code snippets to the traceback.
    let mut traceback = String::new();
    for (_fp, traceback_pc) in runner.vm.get_traceback_entries() {
        if let (0, Some(ref attr)) =
            (traceback_pc.segment_index, get_error_attr_value(traceback_pc.offset, runner))
        {
            traceback.push_str(attr)
        }
        match (traceback_pc.segment_index, get_location(traceback_pc.offset, runner, None)) {
            (0, Some(location)) => {
                traceback.push_str(&format!(
                    "{}\n",
                    location.to_string(&format!("(pc={})", traceback_pc))
                ));
                traceback.push_str(&get_code_snippet(location));
            }
            _ => traceback.push_str(&format!("Unknown location (pc={})\n", traceback_pc)),
        }
    }
    (!traceback.is_empty())
        .then(|| format!("Cairo traceback (most recent call last):\n{traceback}"))
}

/// Gets a code snippet from a file at a specific location.
pub(crate) fn get_code_snippet(location: Location) -> String {
    let path = match location.input_file.filename.split_once("cairo/").map(|(_, rest)| rest) {
        Some(path) => path,
        None => return "Failed to parse input file path.\n".to_string(),
    };

    let file_bytes = match CAIRO_FILES_MAP.get(path) {
        Some(file) => file.as_bytes(),
        None => return format!("File {path} not found in CAIRO_FILES_MAP.\n"),
    };

    let mut snippet = String::new();
    snippet.push_str(&format!("{}\n", location.get_location_marks(file_bytes)));
    snippet
}
