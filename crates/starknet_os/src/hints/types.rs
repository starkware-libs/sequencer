use std::collections::HashMap;

use cairo_vm::hint_processor::hint_processor_definition::{HintProcessor, HintReference};
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::error::{HintExtensionResult, HintResult, OsHintError};

/// Hint enum maps between a (python) hint string in the cairo OS program under cairo-lang to a
/// matching enum variant defined in the crate.
pub trait HintEnum {
    fn from_str(hint_str: &str) -> Result<Self, OsHintError>
    where
        Self: Sized;

    fn to_str(&self) -> &'static str;
}

// TODO(Dori): After hints are implemented, try removing the different lifetime params - probably
//   not all are needed (hopefully only one is needed).
pub struct HintArgs<'vm, 'exec_scopes, 'ids_data, 'ap_tracking, 'constants> {
    pub vm: &'vm mut VirtualMachine,
    pub exec_scopes: &'exec_scopes mut ExecutionScopes,
    pub ids_data: &'ids_data HashMap<String, HintReference>,
    pub ap_tracking: &'ap_tracking ApTracking,
    pub constants: &'constants HashMap<String, Felt>,
}

// TODO(Dori): After hints are implemented, try removing the different lifetime params - probably
//   not all are needed (hopefully only one is needed).
pub struct HintExtensionArgs<'hint_processor, 'vm, 'exec_scopes, 'ids_data, 'ap_tracking> {
    pub hint_processor: &'hint_processor dyn HintProcessor,
    pub vm: &'vm mut VirtualMachine,
    pub exec_scopes: &'exec_scopes mut ExecutionScopes,
    pub ids_data: &'ids_data HashMap<String, HintReference>,
    pub ap_tracking: &'ap_tracking ApTracking,
}

/// Executes the hint logic.
pub trait HintImplementation {
    fn execute_hint(&self, hint_args: HintArgs<'_, '_, '_, '_, '_>) -> HintResult;
}

/// Hint extensions extend the current map of hints used by the VM.
/// This behaviour achieves what the `vm_load_data` primitive does for cairo-lang and is needed to
/// implement OS hints like `vm_load_program`.
pub trait HintExtensionImplementation {
    fn execute_hint_extensive(
        &self,
        hint_extension_args: HintExtensionArgs<'_, '_, '_, '_, '_>,
    ) -> HintExtensionResult;
}
