use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic;
use cairo_vm::stdlib::any::Any;
use cairo_vm::stdlib::boxed::Box;
use cairo_vm::stdlib::collections::HashMap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hints::error::{HintExtensionResult, HintResult};

pub struct SnosHintProcessor<S: StateReader> {
    _execution_helper: OsExecutionHelper<S>,
    _hint_processor: HintProcessor,
    _syscall_hint_processor: SyscallHintProcessor,
    _deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
}

impl<S: StateReader> HintProcessorLogic for SnosHintProcessor<S> {
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

pub(crate) struct HintProcessor;

pub(crate) struct SyscallHintProcessor;

pub(crate) struct DeprecatedSyscallHintProcessor;
