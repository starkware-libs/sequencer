use blockifier::state::cached_state::CachedState;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic;
use cairo_vm::stdlib::any::Any;
use cairo_vm::stdlib::boxed::Box;
use cairo_vm::stdlib::collections::HashMap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hint_processor::panicking_state_reader::PanickingStateReader;
use crate::hints::error::{HintExtensionResult, HintResult};

// TODO(Nimrod): Make it generic with S: StateReader.
pub struct SnosHintProcessor {
    pub execution_helper: OsExecutionHelper<CachedState<PanickingStateReader>>,
}

impl HintProcessorLogic for SnosHintProcessor {
    fn execute_hint(
        &mut self,
        _vm: &mut VirtualMachine,
        _exec_scopes: &mut ExecutionScopes,
        _hint_data: &Box<dyn Any>,
        _constants: &HashMap<String, Felt>,
    ) -> HintResult {
        Ok(())
    }

    fn execute_hint_extensive(
        &mut self,
        _vm: &mut VirtualMachine,
        _exec_scopes: &mut ExecutionScopes,
        _hint_data: &Box<dyn Any>,
        _constants: &HashMap<String, Felt>,
    ) -> HintExtensionResult {
        todo!()
    }
}
