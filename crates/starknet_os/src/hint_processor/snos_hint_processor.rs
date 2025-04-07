use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::{
    BuiltinHintProcessor,
    HintProcessorData,
};
use cairo_vm::hint_processor::hint_processor_definition::{HintExtension, HintProcessorLogic};
use cairo_vm::stdlib::any::Any;
use cairo_vm::stdlib::boxed::Box;
use cairo_vm::stdlib::collections::HashMap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::hint_processor::execution_helper::{ExecutionHelperError, OsExecutionHelper};
use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::enum_definition::AllHints;
use crate::hints::error::OsHintError;
use crate::hints::types::{HintArgs, HintEnum, HintExtensionImplementation, HintImplementation};
#[cfg(any(feature = "testing", test))]
use crate::io::os_input::{CachedStateInput, OsBlockInput, StarknetOsInput};
use crate::io::os_input::{OsHints, OsHintsConfig, OsInputError};

type VmHintResultType<T> = Result<T, VmHintError>;
type VmHintResult = VmHintResultType<()>;
type VmHintExtensionResult = VmHintResultType<HintExtension>;

pub(crate) struct ExecutionHelpersManager<S: StateReader> {
    execution_helpers: Vec<OsExecutionHelper<S>>,
    current_index: Option<usize>,
}

impl<S: StateReader> ExecutionHelpersManager<S> {
    pub fn new(execution_helpers: Vec<OsExecutionHelper<S>>) -> Self {
        Self { execution_helpers, current_index: None }
    }

    /// Returns an execution helper reference of the currently processed block.
    pub fn get_current_execution_helper(
        &self,
    ) -> Result<&OsExecutionHelper<S>, ExecutionHelperError> {
        let current_idx = self.get_current_index()?;
        Ok(self
            .execution_helpers
            .get(current_idx)
            .expect("Current execution helper index is out of bounds."))
    }

    /// Returns an execution helper mutable reference of the currently processed block.
    pub fn get_mut_current_execution_helper(
        &mut self,
    ) -> Result<&mut OsExecutionHelper<S>, ExecutionHelperError> {
        let current_idx = self.get_current_index()?;
        Ok(self
            .execution_helpers
            .get_mut(current_idx)
            .expect("Current execution helper index is out of bounds."))
    }

    #[allow(dead_code)]
    /// Increments the current helper index.
    pub fn increment_current_helper_index(&mut self) {
        self.current_index = match self.current_index {
            Some(prev_idx) => Some(prev_idx + 1),
            None => Some(0),
        };
    }

    /// Returns the current helper index.
    fn get_current_index(&self) -> Result<usize, ExecutionHelperError> {
        self.current_index.ok_or(ExecutionHelperError::NoCurrentExecutionHelper)
    }
}

pub struct SnosHintProcessor<S: StateReader> {
    pub(crate) os_program: Program,
    pub(crate) execution_helpers_manager: ExecutionHelpersManager<S>,
    pub(crate) os_hints_config: OsHintsConfig,
    pub syscall_hint_processor: SyscallHintProcessor,
    pub(crate) deprecated_compiled_classes: HashMap<ClassHash, ContractClass>,
    pub(crate) compiled_classes: HashMap<ClassHash, CasmContractClass>,
    pub(crate) state_update_pointers: Option<StateUpdatePointers>,
    _deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
    builtin_hint_processor: BuiltinHintProcessor,
    // KZG fields.
    da_segment: Option<Vec<Felt>>,
}

impl<S: StateReader> SnosHintProcessor<S> {
    pub fn new(
        os_program: Program,
        os_hints: OsHints,
        state_readers: Vec<S>,
        syscall_hint_processor: SyscallHintProcessor,
        deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
    ) -> Result<Self, StarknetOsError> {
        if state_readers.len() != os_hints.os_input.os_block_and_state_input.len() {
            return Err(OsInputError::InvalidLengthOfStateReaders(
                state_readers.len(),
                os_hints.os_input.os_block_and_state_input.len(),
            )
            .into());
        }
        let execution_helpers = os_hints
            .os_input
            .os_block_and_state_input
            .into_iter()
            .zip(state_readers.into_iter())
            .map(|((os_block_input, cached_state_input), state_reader)| {
                OsExecutionHelper::new(
                    os_block_input,
                    state_reader,
                    cached_state_input,
                    os_hints.os_hints_config.debug_mode,
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            os_program,
            execution_helpers_manager: ExecutionHelpersManager::new(execution_helpers),
            os_hints_config: os_hints.os_hints_config,
            syscall_hint_processor,
            _deprecated_syscall_hint_processor: deprecated_syscall_hint_processor,
            da_segment: None,
            builtin_hint_processor: BuiltinHintProcessor::new_empty(),
            deprecated_compiled_classes: os_hints.os_input.deprecated_compiled_classes,
            compiled_classes: os_hints.os_input.compiled_classes,
            state_update_pointers: None,
        })
    }

    /// Stores the data-availabilty segment, to be used for computing the KZG commitment in blob
    /// mode.
    pub(crate) fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> Result<(), OsHintError> {
        if self.da_segment.is_some() {
            return Err(OsHintError::AssertionFailed {
                message: "DA segment is already initialized.".to_string(),
            });
        }
        self.da_segment = Some(da_segment);
        Ok(())
    }

    /// Returns an execution helper reference of the currently processed block.
    pub fn get_current_execution_helper(
        &self,
    ) -> Result<&OsExecutionHelper<S>, ExecutionHelperError> {
        self.execution_helpers_manager.get_current_execution_helper()
    }

    /// Returns an execution helper mutable reference of the currently processed block.
    pub fn get_mut_current_execution_helper(
        &mut self,
    ) -> Result<&mut OsExecutionHelper<S>, ExecutionHelperError> {
        self.execution_helpers_manager.get_mut_current_execution_helper()
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
        // OS hint, aggregator hint, Cairo0 syscall or Cairo0 core hint.
        if let Some(hint_processor_data) = hint_data.downcast_ref::<HintProcessorData>() {
            let hint_args = HintArgs {
                hint_processor: self,
                vm,
                exec_scopes,
                ids_data: &hint_processor_data.ids_data,
                ap_tracking: &hint_processor_data.ap_tracking,
                constants,
            };
            if let Ok(hint) = AllHints::from_str(hint_processor_data.code.as_str()) {
                // OS hint, aggregator hint, Cairo0 syscall.
                return match hint {
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
            } else {
                // Cairo0 core hint.
                self.builtin_hint_processor.execute_hint(vm, exec_scopes, hint_data, constants)?;
                return Ok(HintExtension::default());
            }
        }

        // Cairo1 syscall or Cairo1 core hint.
        todo!()
    }
}

#[cfg(any(test, feature = "testing"))]
impl SnosHintProcessor<DictStateReader> {
    pub fn new_for_testing(
        state_reader: Option<DictStateReader>,
        os_program: Option<Program>,
        os_hints_config: Option<OsHintsConfig>,
        os_block_input: Option<(OsBlockInput, CachedStateInput)>,
    ) -> Result<Self, StarknetOsError> {
        let state_reader = state_reader.unwrap_or_default();
        let os_program = os_program.unwrap_or_default();
        let os_hints_config = os_hints_config.unwrap_or_default();
        let os_block_input = os_block_input.unwrap_or_default();
        let os_input = StarknetOsInput {
            os_block_and_state_input: vec![os_block_input],
            ..Default::default()
        };
        let os_hints = OsHints { os_input, os_hints_config };
        let syscall_handler = SyscallHintProcessor::new();
        let deprecated_syscall_handler = DeprecatedSyscallHintProcessor {};

        SnosHintProcessor::new(
            os_program,
            os_hints,
            vec![state_reader],
            syscall_handler,
            deprecated_syscall_handler,
        )
    }
}

/// Default implementation (required for the VM to use the type as a hint processor).
impl<S: StateReader> ResourceTracker for SnosHintProcessor<S> {}

pub struct SyscallHintProcessor {
    // Sha256 segments.
    sha256_segment: Option<Relocatable>,
    syscall_ptr: Option<Relocatable>,
}

// TODO(Dori): remove this #[allow] after the constructor is no longer trivial.
#[allow(clippy::new_without_default)]
impl SyscallHintProcessor {
    pub fn new() -> Self {
        Self { sha256_segment: None, syscall_ptr: None }
    }

    pub fn set_sha256_segment(&mut self, sha256_segment: Relocatable) {
        self.sha256_segment = Some(sha256_segment);
    }

    pub fn set_syscall_ptr(&mut self, syscall_ptr: Relocatable) {
        self.syscall_ptr = Some(syscall_ptr);
    }
}

pub struct DeprecatedSyscallHintProcessor;
