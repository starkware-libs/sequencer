use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::HintProcessorData;
use cairo_vm::hint_processor::hint_processor_definition::{HintExtension, HintProcessorLogic};
use cairo_vm::stdlib::any::Any;
use cairo_vm::stdlib::boxed::Box;
use cairo_vm::stdlib::collections::HashMap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hints::enum_definition::AllHints;
use crate::hints::types::{HintArgs, HintEnum, HintExtensionImplementation, HintImplementation};

type VmHintResultType<T> = Result<T, VmHintError>;
type VmHintResult = VmHintResultType<()>;
type VmHintExtensionResult = VmHintResultType<HintExtension>;

pub struct SnosHintProcessor<S: StateReader> {
    pub execution_helper: OsExecutionHelper<S>,
    pub syscall_hint_processor: SyscallHintProcessor,
    _deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
}

impl<S: StateReader> HintProcessorLogic for SnosHintProcessor<S> {
    fn execute_hint(
        &mut self,
        _vm: &mut VirtualMachine,
        _exec_scopes: &mut ExecutionScopes,
        _hint_data: &Box<dyn Any>,
        _constants: &HashMap<String, Felt>,
    ) -> VmHintResult {
        Ok(())
    }

    fn execute_hint_extensive(
        &mut self,
        vm: &mut VirtualMachine,
        exec_scopes: &mut ExecutionScopes,
        hint_data: &Box<dyn Any>,
        constants: &HashMap<String, Felt>,
    ) -> VmHintExtensionResult {
        // OS hint, aggregator hint or Cairo0 syscall.
        if let Some(hint_processor_data) = hint_data.downcast_ref::<HintProcessorData>() {
            let hint_args = HintArgs {
                hint_processor: self,
                vm,
                exec_scopes,
                ids_data: &hint_processor_data.ids_data,
                ap_tracking: &hint_processor_data.ap_tracking,
                constants,
            };
            return match AllHints::from_str(hint_processor_data.code.as_str())? {
                AllHints::OsHint(os_hint) => {
                    os_hint.execute_hint(hint_args)?;
                    Ok(HintExtension::default())
                }
                AllHints::AggregatorHint(aggregator_hint) => {
                    aggregator_hint.execute_hint(hint_args)?;
                    Ok(HintExtension::default())
                }
                AllHints::SyscallHint(syscall_hint) => {
                    syscall_hint.execute_hint(hint_args)?;
                    Ok(HintExtension::default())
                }
                AllHints::HintExtension(hint_extension) => {
                    Ok(hint_extension.execute_hint_extensive(hint_args)?)
                }
            };
        }

        // Cairo1 syscall or core hint.
        todo!()
    }
}

/// Default implementation (required for the VM to use the type as a hint processor).
impl<S: StateReader> ResourceTracker for SnosHintProcessor<S> {}

pub struct SyscallHintProcessor {
    // Sha256 segments.
    sha256_segment: Option<Relocatable>,
}

impl SyscallHintProcessor {
    pub fn set_sha256_segment(&mut self, sha256_segment: Relocatable) {
        self.sha256_segment = Some(sha256_segment);
    }
}

pub(crate) struct DeprecatedSyscallHintProcessor;
