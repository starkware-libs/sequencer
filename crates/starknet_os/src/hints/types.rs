use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};

/// Hint enum maps between a (python) hint string in the cairo OS program under cairo-lang to a
/// matching enum variant defined in the crate.
pub trait HintEnum {
    #[allow(clippy::result_large_err)]
    fn from_str(hint_str: &str) -> Result<Self, OsHintError>
    where
        Self: Sized;

    fn to_str(&self) -> &'static str;
}

pub struct HintArgs<'a, 'program, S: StateReader> {
    pub hint_processor: &'a mut SnosHintProcessor<'program, S>,
    pub vm: &'a mut VirtualMachine,
    pub exec_scopes: &'a mut ExecutionScopes,
    pub ids_data: &'a HashMap<String, HintReference>,
    pub ap_tracking: &'a ApTracking,
    pub constants: &'a HashMap<String, Felt>,
}

/// Executes the hint logic.
pub trait HintImplementation {
    #[allow(clippy::result_large_err)]
    fn execute_hint<S: StateReader>(&self, hint_args: HintArgs<'_, '_, S>) -> OsHintResult;
}

/// Hint extensions extend the current map of hints used by the VM.
/// This behaviour achieves what the `vm_load_data` primitive does for cairo-lang and is needed to
/// implement OS hints like `vm_load_program`.
pub trait HintExtensionImplementation {
    #[allow(clippy::result_large_err)]
    fn execute_hint_extensive<S: StateReader>(
        &self,
        hint_extension_args: HintArgs<'_, '_, S>,
    ) -> OsHintExtensionResult;
}
