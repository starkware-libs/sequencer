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
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintError;

/// Hint enum maps between a (python) hint string in the cairo OS program under cairo-lang to a
/// matching enum variant defined in the crate.
pub trait HintEnum {
    fn from_str(hint_str: &str) -> Result<Self, OsHintError>
    where
        Self: Sized;

    fn to_str(&self) -> &'static str;
}

pub struct HintArgs<'a> {
    pub vm: &'a mut VirtualMachine,
    pub exec_scopes: &'a mut ExecutionScopes,
    pub ids_data: &'a HashMap<String, HintReference>,
    pub ap_tracking: &'a ApTracking,
    pub constants: &'a HashMap<String, Felt>,
}

impl HintArgs<'_> {
    pub fn insert_value<T: Into<MaybeRelocatable>>(
        &mut self,
        var_name: &str,
        value: T,
    ) -> Result<(), HintError> {
        insert_value_from_var_name(var_name, value, self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_integer(&self, var_name: &str) -> Result<Felt, HintError> {
        get_integer_from_var_name(var_name, self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_ptr(&self, var_name: &str) -> Result<Relocatable, HintError> {
        get_ptr_from_var_name(var_name, self.vm, self.ids_data, self.ap_tracking)
    }

    pub fn get_relocatable(&self, var_name: &str) -> Result<Relocatable, HintError> {
        get_relocatable_from_var_name(var_name, self.vm, self.ids_data, self.ap_tracking)
    }
}
