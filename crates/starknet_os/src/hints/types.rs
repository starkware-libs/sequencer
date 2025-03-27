use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::hint_processor_definition::{HintExtension, HintReference};
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::enum_definition::AllHints;
use crate::hints::error::OsHintError;

#[derive(Debug, thiserror::Error)]
pub enum HintImplementationError {
    #[error("Encountered error executing hint '{hint:?}': {error:?}.")]
    OsHint { hint: AllHints, error: OsHintError },
    #[error("Unknown hint string: {0}")]
    UnknownHint(String),
}

/// `HintImplementationError` and the VM's `HintError` must have conversions in both directions, as
/// execution can pass back and forth between the VM and the OS hint processor; errors should
/// propagate.
// TODO(Dori): Consider replicating the blockifier's mechanism and keeping structured error data,
//   instead of converting to string.
impl From<HintImplementationError> for VmHintError {
    fn from(error: HintImplementationError) -> Self {
        Self::CustomHint(format!("{error}").into())
    }
}

pub type HintImplementationResult = Result<(), HintImplementationError>;
pub type HintExtensionImplementationResult = Result<HintExtension, HintImplementationError>;

/// Hint enum maps between a (python) hint string in the cairo OS program under cairo-lang to a
/// matching enum variant defined in the crate.
pub trait HintEnum {
    fn from_str(hint_str: &str) -> Result<Self, HintImplementationError>
    where
        Self: Sized;

    fn to_str(&self) -> &'static str;
}

pub struct HintArgs<'a, S: StateReader> {
    pub hint_processor: &'a mut SnosHintProcessor<S>,
    pub vm: &'a mut VirtualMachine,
    pub exec_scopes: &'a mut ExecutionScopes,
    pub ids_data: &'a HashMap<String, HintReference>,
    pub ap_tracking: &'a ApTracking,
    pub constants: &'a HashMap<String, Felt>,
}

/// Executes the hint logic.
pub trait HintImplementation {
    fn execute_hint<S: StateReader>(&self, hint_args: HintArgs<'_, S>) -> HintImplementationResult;
}

/// Hint extensions extend the current map of hints used by the VM.
/// This behaviour achieves what the `vm_load_data` primitive does for cairo-lang and is needed to
/// implement OS hints like `vm_load_program`.
pub trait HintExtensionImplementation {
    fn execute_hint_extensive<S: StateReader>(
        &self,
        hint_extension_args: HintArgs<'_, S>,
    ) -> HintExtensionImplementationResult;
}
