use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    get_relocatable_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::{get_address_of_nested_fields, insert_values_to_fields, VmUtilsResult};

/// Hint enum maps between a (python) hint string in the cairo OS program under cairo-lang to a
/// matching enum variant defined in the crate.
pub trait HintEnum {
    fn from_str(hint_str: &str) -> Result<Self, OsHintError>
    where
        Self: Sized;

    fn to_str(&self) -> &'static str;
}

pub struct HintContext<'a> {
    pub vm: &'a mut VirtualMachine,
    pub exec_scopes: &'a mut ExecutionScopes,
    pub ids_data: &'a HashMap<String, HintReference>,
    pub ap_tracking: &'a ApTracking,
    pub program: &'a Program,
}

impl HintContext<'_> {
    pub fn insert_value<T: Into<MaybeRelocatable>>(
        &mut self,
        var_id: Ids,
        value: T,
    ) -> Result<(), HintError> {
        insert_value_from_var_name(var_id.into(), value, self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_integer(&self, var_id: Ids) -> Result<Felt, HintError> {
        get_integer_from_var_name(var_id.into(), self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_ptr(&self, var_id: Ids) -> Result<Relocatable, HintError> {
        get_ptr_from_var_name(var_id.into(), self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_relocatable(&self, var_id: Ids) -> Result<Relocatable, HintError> {
        get_relocatable_from_var_name(var_id.into(), self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn fetch_as<T: TryFrom<Felt>>(&self, var_id: Ids) -> Result<T, OsHintError>
    where
        <T as TryFrom<Felt>>::Error: std::fmt::Debug,
    {
        var_id.fetch_as(self.vm, self.ids_data, self.ap_tracking)
    }

    /// Returns a reference to the program's constants.
    pub fn constants(&self) -> &HashMap<String, Felt> {
        &self.program.constants
    }

    pub fn fetch_const(&self, constant: Const) -> Result<&Felt, HintError> {
        constant.fetch(self.constants())
    }

    pub fn fetch_const_as<T: TryFrom<Felt>>(&self, constant: Const) -> Result<T, OsHintError>
    where
        <T as TryFrom<Felt>>::Error: std::fmt::Debug,
    {
        constant.fetch_as(self.constants())
    }

    /// Inserts each value to a field of a cairo variable given a base address.
    pub fn insert_to_fields(
        &mut self,
        base_address: Relocatable,
        var_type: CairoStruct,
        fields_and_values: &[(&str, MaybeRelocatable)],
    ) -> VmUtilsResult<()> {
        insert_values_to_fields(base_address, var_type, self.vm, fields_and_values, self.program)
    }

    /// Fetches the address of nested fields of a cairo variable.
    pub fn get_nested_field_address(
        &self,
        id: Ids,
        var_type: CairoStruct,
        nested_fields: &[&str],
    ) -> VmUtilsResult<Relocatable> {
        get_address_of_nested_fields(
            self.ids_data,
            id,
            var_type,
            self.vm,
            self.ap_tracking,
            nested_fields,
            self.program,
        )
    }

    /// Gets a Felt from a nested field of a cairo variable.
    pub fn get_nested_field_felt(
        &self,
        id: Ids,
        var_type: CairoStruct,
        nested_fields: &[&str],
    ) -> VmUtilsResult<Felt> {
        let address = self.get_nested_field_address(id, var_type, nested_fields)?;
        Ok(self.vm.get_integer(address)?.into_owned())
    }

    /// Gets a pointer (Relocatable) from a nested field of a cairo variable.
    pub fn get_nested_field_ptr(
        &self,
        id: Ids,
        var_type: CairoStruct,
        nested_fields: &[&str],
    ) -> VmUtilsResult<Relocatable> {
        let address = self.get_nested_field_address(id, var_type, nested_fields)?;
        Ok(self.vm.get_relocatable(address)?)
    }
}
