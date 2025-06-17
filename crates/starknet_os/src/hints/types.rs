use std::collections::HashMap;

use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
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
