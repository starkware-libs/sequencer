use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::HintProcessorData;
use cairo_vm::hint_processor::hint_processor_definition::{HintExtension, HintProcessorLogic};
use cairo_vm::stdlib::any::Any;
use cairo_vm::stdlib::boxed::Box;
use cairo_vm::stdlib::collections::HashMap;
use cairo_vm::types::exec_scope::ExecutionScopes;
#[cfg(any(feature = "testing", test))]
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hints::enum_definition::AllHints;
use crate::hints::error::OsHintError;
use crate::hints::types::{HintArgs, HintEnum, HintExtensionImplementation, HintImplementation};
#[cfg(any(feature = "testing", test))]
use crate::io::os_input::StarknetOsInput;

type VmHintResultType<T> = Result<T, VmHintError>;
type VmHintResult = VmHintResultType<()>;
type VmHintExtensionResult = VmHintResultType<HintExtension>;

pub struct SnosHintProcessor<S: StateReader> {
    pub execution_helper: OsExecutionHelper<S>,
    pub syscall_hint_processor: SyscallHintProcessor,
    _deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,

    // KZG fields.
    da_segment: Option<Vec<Felt>>,
}

impl<S: StateReader> SnosHintProcessor<S> {
    pub fn new(
        execution_helper: OsExecutionHelper<S>,
        syscall_hint_processor: SyscallHintProcessor,
        deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
    ) -> Self {
        Self {
            execution_helper,
            syscall_hint_processor,
            _deprecated_syscall_hint_processor: deprecated_syscall_hint_processor,
            da_segment: None,
        }
    }

    /// Stores the data-availabilty segment, to be used for computing the KZG commitment in blob
    /// mode.
    #[allow(dead_code)]
    pub(crate) fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> Result<(), OsHintError> {
        if self.da_segment.is_some() {
            return Err(OsHintError::AssertionFailed {
                message: "DA segment is already initialized.".to_string(),
            });
        }
        self.da_segment = Some(da_segment);
        Ok(())
    }
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

#[cfg(any(test, feature = "testing"))]
impl SnosHintProcessor<DictStateReader> {
    pub fn new_for_testing(
        state_reader: Option<DictStateReader>,
        os_input: Option<StarknetOsInput>,
        os_program: Option<Program>,
    ) -> Self {
        let state_reader = state_reader.unwrap_or_default();
        let os_input = os_input.unwrap_or_default();
        let os_program = os_program.unwrap_or_default();
        let execution_helper = OsExecutionHelper::<DictStateReader>::new_for_testing(
            state_reader,
            os_input,
            os_program,
        );

        let syscall_handler = SyscallHintProcessor::new();
        let deprecated_syscall_handler = DeprecatedSyscallHintProcessor {};

        SnosHintProcessor::new(execution_helper, syscall_handler, deprecated_syscall_handler)
    }
}

/// Default implementation (required for the VM to use the type as a hint processor).
impl<S: StateReader> ResourceTracker for SnosHintProcessor<S> {}

pub struct SyscallHintProcessor {
    // Sha256 segments.
    sha256_segment: Option<Relocatable>,
}

// TODO(Dori): remove this #[allow] after the constructor is no longer trivial.
#[allow(clippy::new_without_default)]
impl SyscallHintProcessor {
    pub fn new() -> Self {
        Self { sha256_segment: None }
    }

    pub fn set_sha256_segment(&mut self, sha256_segment: Relocatable) {
        self.sha256_segment = Some(sha256_segment);
    }
}

pub struct DeprecatedSyscallHintProcessor;
