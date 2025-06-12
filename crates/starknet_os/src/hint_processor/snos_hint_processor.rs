use std::collections::btree_map::IntoIter;
use std::collections::{BTreeMap, HashSet};

use blockifier::execution::call_info::CallExecution;
use blockifier::execution::syscalls::secp::SecpHintProcessor;
use blockifier::execution::syscalls::vm_syscall_utils::{execute_next_syscall, SyscallUsageMap};
use blockifier::state::state_api::StateReader;
#[cfg(any(feature = "testing", test))]
use blockifier::test_utils::dict_state_reader::DictStateReader;
use cairo_lang_casm::hints::Hint as Cairo1Hint;
use cairo_lang_runner::casm_run::execute_core_hint_base;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::{
    BuiltinHintProcessor,
    HintProcessorData as Cairo0Hint,
};
use cairo_vm::hint_processor::hint_processor_definition::{HintExtension, HintProcessorLogic};
use cairo_vm::stdlib::any::Any;
use cairo_vm::stdlib::boxed::Box;
use cairo_vm::stdlib::collections::HashMap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::{HintError, HintError as VmHintError};
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_types_core::felt::Felt;

use crate::errors::StarknetOsError;
use crate::hint_processor::execution_helper::{ExecutionHelperError, OsExecutionHelper};
use crate::hint_processor::state_update_pointers::StateUpdatePointers;
#[cfg(any(test, feature = "testing"))]
use crate::hint_processor::test_hint::test_hint;
use crate::hints::enum_definition::AllHints;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::state::CommitmentType;
use crate::hints::types::{HintArgs, HintArgsNoHP, HintEnum};
use crate::io::os_input::{
    CachedStateInput,
    CommitmentInfo,
    OsBlockInput,
    OsHintsConfig,
    OsInputError,
};

type VmHintResultType<T> = Result<T, VmHintError>;
type VmHintResult = VmHintResultType<()>;
type VmHintExtensionResult = VmHintResultType<HintExtension>;

pub(crate) struct ExecutionHelpersManager<'a, S: StateReader> {
    execution_helpers: Vec<OsExecutionHelper<'a, S>>,
    current_index: Option<usize>,
}

impl<'a, S: StateReader> ExecutionHelpersManager<'a, S> {
    pub fn new(execution_helpers: Vec<OsExecutionHelper<'a, S>>) -> Self {
        Self { execution_helpers, current_index: None }
    }

    /// Returns an execution helper reference of the currently processed block.
    pub fn get_current_execution_helper(
        &self,
    ) -> Result<&OsExecutionHelper<'a, S>, ExecutionHelperError> {
        let current_idx = self.get_current_index()?;
        Ok(self
            .execution_helpers
            .get(current_idx)
            .expect("Current execution helper index is out of bounds."))
    }

    /// Returns an execution helper mutable reference of the currently processed block.
    pub fn get_mut_current_execution_helper(
        &mut self,
    ) -> Result<&mut OsExecutionHelper<'a, S>, ExecutionHelperError> {
        let current_idx = self.get_current_index()?;
        Ok(self
            .execution_helpers
            .get_mut(current_idx)
            .expect("Current execution helper index is out of bounds."))
    }

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

    /// Returns the number of execution helpers.
    pub fn n_helpers(&self) -> usize {
        self.execution_helpers.len()
    }
}

pub struct SnosHintProcessor<'a, S: StateReader> {
    // The program being run. The hint processor does not require ownership.
    pub(crate) os_program: &'a Program,
    pub(crate) execution_helpers_manager: ExecutionHelpersManager<'a, S>,
    pub(crate) os_hints_config: OsHintsConfig,
    pub syscall_hint_processor: SyscallHintProcessor,
    pub(crate) deprecated_compiled_classes_iter: IntoIter<ClassHash, ContractClass>,
    pub(crate) deprecated_class_hashes: HashSet<ClassHash>,
    pub(crate) compiled_classes: BTreeMap<ClassHash, CasmContractClass>,
    pub(crate) state_update_pointers: Option<StateUpdatePointers>,
    pub(crate) deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
    builtin_hint_processor: BuiltinHintProcessor,
    // The type of commitment tree next in line for hashing. Used to determine which HashBuiltin
    // type is to be used.
    pub(crate) commitment_type: CommitmentType,
    // KZG fields.
    da_segment: Option<Vec<Felt>>,
    // Indicates wether to create pages or not when serializing data-availability.
    pub(crate) serialize_data_availability_create_pages: bool,
    // For testing, track hint coverage.
    #[cfg(feature = "testing")]
    pub unused_hints: HashSet<AllHints>,
}

impl<'a, S: StateReader> SnosHintProcessor<'a, S> {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::result_large_err)]
    pub fn new(
        os_program: &'a Program,
        os_hints_config: OsHintsConfig,
        os_block_inputs: Vec<&'a OsBlockInput>,
        cached_state_inputs: Vec<CachedStateInput>,
        deprecated_compiled_classes: BTreeMap<ClassHash, ContractClass>,
        compiled_classes: BTreeMap<ClassHash, CasmContractClass>,
        state_readers: Vec<S>,
        syscall_hint_processor: SyscallHintProcessor,
        deprecated_syscall_hint_processor: DeprecatedSyscallHintProcessor,
    ) -> Result<Self, StarknetOsError> {
        if state_readers.len() != os_block_inputs.len() {
            return Err(OsInputError::InvalidLengthOfStateReaders(
                state_readers.len(),
                os_block_inputs.len(),
            )
            .into());
        }
        let execution_helpers = os_block_inputs
            .into_iter()
            .zip(cached_state_inputs.into_iter())
            .zip(state_readers.into_iter())
            .map(|((os_block_input, cached_state_input), state_reader)| {
                OsExecutionHelper::new(
                    os_block_input,
                    state_reader,
                    cached_state_input,
                    os_hints_config.debug_mode,
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            os_program,
            execution_helpers_manager: ExecutionHelpersManager::new(execution_helpers),
            os_hints_config,
            syscall_hint_processor,
            deprecated_syscall_hint_processor,
            da_segment: None,
            builtin_hint_processor: BuiltinHintProcessor::new_empty(),
            deprecated_class_hashes: deprecated_compiled_classes.keys().copied().collect(),
            deprecated_compiled_classes_iter: deprecated_compiled_classes.into_iter(),
            compiled_classes,
            state_update_pointers: None,
            commitment_type: CommitmentType::State,
            serialize_data_availability_create_pages: false,
            #[cfg(feature = "testing")]
            unused_hints: AllHints::all_iter().collect(),
        })
    }

    /// Stores the data-availabilty segment, to be used for computing the KZG commitment in blob
    /// mode.
    #[allow(clippy::result_large_err)]
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
    ) -> Result<&OsExecutionHelper<'a, S>, ExecutionHelperError> {
        self.execution_helpers_manager.get_current_execution_helper()
    }

    /// Returns an execution helper mutable reference of the currently processed block.
    pub fn get_mut_current_execution_helper(
        &mut self,
    ) -> Result<&mut OsExecutionHelper<'a, S>, ExecutionHelperError> {
        self.execution_helpers_manager.get_mut_current_execution_helper()
    }

    /// Returns the number of blocks executed by the OS.
    pub fn n_blocks(&self) -> usize {
        // Each execution helper corresponds to a block.
        self.execution_helpers_manager.n_helpers()
    }

    pub fn get_next_call_execution(&mut self) -> Result<&CallExecution, ExecutionHelperError> {
        Ok(&self
            .execution_helpers_manager
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?
            .next_inner_call()?
            .execution)
    }

    pub fn get_commitment_info(&self) -> Result<&CommitmentInfo, ExecutionHelperError> {
        let os_input = self.get_current_execution_helper()?.os_block_input;
        Ok(match self.commitment_type {
            CommitmentType::Class => &os_input.contract_class_commitment_info,
            CommitmentType::State => &os_input.contract_state_commitment_info,
        })
    }
}

impl<S: StateReader> HintProcessorLogic for SnosHintProcessor<'_, S> {
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
        if let Some(hint_processor_data) = hint_data.downcast_ref::<Cairo0Hint>() {
            // AllHints (OS hint, aggregator hint, Cairo0 syscall) or Cairo0 core hint.
            let hint_args = HintArgs {
                vm,
                exec_scopes,
                ids_data: &hint_processor_data.ids_data,
                ap_tracking: &hint_processor_data.ap_tracking,
                constants,
            };
            if let Ok(hint) = AllHints::from_str(hint_processor_data.code.as_str()) {
                // OS hint, aggregator hint, Cairo0 syscall.
                return match hint {
                    AllHints::StatelessHint(stateless) => {
                        stateless.execute_hint(
                            self,
                            HintArgsNoHP {
                                vm,
                                exec_scopes,
                                ids_data: &hint_processor_data.ids_data,
                                ap_tracking: &hint_processor_data.ap_tracking,
                                constants,
                            },
                        )?;
                        Ok(HintExtension::default())
                    }
                    AllHints::OsHint(os_hint) => {
                        os_hint.execute_hint(self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                    AllHints::AggregatorHint(aggregator_hint) => {
                        aggregator_hint.execute_hint(self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                    AllHints::DeprecatedSyscallHint(deprecated_syscall_hint) => {
                        deprecated_syscall_hint.execute_hint(self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                    AllHints::HintExtension(hint_extension) => {
                        Ok(hint_extension.execute_hint_extensive(self, hint_args)?)
                    }
                    #[cfg(any(test, feature = "testing"))]
                    AllHints::TestHint => {
                        test_hint(hint_processor_data.code.as_str(), self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                };
            } else {
                // Cairo0 core hint.
                self.builtin_hint_processor.execute_hint(vm, exec_scopes, hint_data, constants)?;
                return Ok(HintExtension::default());
            }
        }

        // Cairo1 syscall or Cairo1 core hint.
        match hint_data.downcast_ref::<Cairo1Hint>().ok_or(HintError::WrongHintData)? {
            Cairo1Hint::Core(hint) => {
                let no_temporary_segments = true;
                execute_core_hint_base(vm, exec_scopes, hint, no_temporary_segments)?;
                Ok(HintExtension::default())
            }
            Cairo1Hint::Starknet(hint) => {
                execute_next_syscall(self, vm, hint)?;
                Ok(HintExtension::default())
            }
            Cairo1Hint::External(_) => {
                panic!("starknet should never accept classes with external hints!")
            }
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl<'a> SnosHintProcessor<'a, DictStateReader> {
    #[allow(clippy::result_large_err)]
    pub fn new_for_testing(
        state_reader: Option<DictStateReader>,
        os_program: &'a Program,
        os_hints_config: Option<OsHintsConfig>,
        os_block_input: &'a OsBlockInput,
        os_state_input: Option<CachedStateInput>,
    ) -> Result<Self, StarknetOsError> {
        let state_reader = state_reader.unwrap_or_default();
        let block_inputs = vec![os_block_input];
        let state_inputs = vec![os_state_input.unwrap_or_default()];
        let os_hints_config = os_hints_config.unwrap_or_default();
        let syscall_handler = SyscallHintProcessor::new();
        let deprecated_syscall_handler = DeprecatedSyscallHintProcessor::new();

        SnosHintProcessor::new(
            os_program,
            os_hints_config,
            block_inputs,
            state_inputs,
            BTreeMap::new(),
            BTreeMap::new(),
            vec![state_reader],
            syscall_handler,
            deprecated_syscall_handler,
        )
    }
}

/// Default implementation (required for the VM to use the type as a hint processor).
impl<S: StateReader> ResourceTracker for SnosHintProcessor<'_, S> {}

pub struct SyscallHintProcessor {
    // Sha256 segments.
    pub(crate) sha256_segment_end_ptr: Option<Relocatable>,
    syscall_ptr: Option<Relocatable>,
    pub(crate) syscall_usage: SyscallUsageMap,

    // Secp hint processors.
    pub(crate) secp256k1_hint_processor: SecpHintProcessor<ark_secp256k1::Config>,
    pub(crate) secp256r1_hint_processor: SecpHintProcessor<ark_secp256r1::Config>,
}

// TODO(Dori): remove this #[allow] after the constructor is no longer trivial.
#[allow(clippy::new_without_default)]
impl SyscallHintProcessor {
    pub fn new() -> Self {
        Self {
            sha256_segment_end_ptr: None,
            syscall_ptr: None,
            secp256k1_hint_processor: SecpHintProcessor::default(),
            secp256r1_hint_processor: SecpHintProcessor::default(),
            syscall_usage: SyscallUsageMap::new(),
        }
    }

    pub fn set_syscall_ptr(&mut self, syscall_ptr: Relocatable) {
        self.syscall_ptr = Some(syscall_ptr);
    }

    #[allow(clippy::result_large_err)]
    pub fn validate_and_discard_syscall_ptr(
        &mut self,
        syscall_ptr_end: &Relocatable,
    ) -> OsHintResult {
        match &self.syscall_ptr {
            Some(syscall_ptr) if syscall_ptr == syscall_ptr_end => {
                self.syscall_ptr = None;
                Ok(())
            }
            Some(_) => {
                Err(OsHintError::AssertionFailed { message: "Bad syscall_ptr_end.".to_string() })
            }
            None => Err(OsHintError::AssertionFailed { message: "Missing syscall_ptr.".into() }),
        }
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn get_mut_syscall_ptr(&mut self) -> Result<&mut Relocatable, OsHintError> {
        self.syscall_ptr.as_mut().ok_or(OsHintError::UnsetSyscallPtr)
    }
}

pub struct DeprecatedSyscallHintProcessor {
    pub(crate) syscall_ptr: Option<Relocatable>,
    pub(crate) syscalls_usage: SyscallUsageMap,
}

// TODO(Dori): remove this #[allow] after the constructor is no longer trivial.
#[allow(clippy::new_without_default)]
impl DeprecatedSyscallHintProcessor {
    pub fn new() -> Self {
        Self { syscall_ptr: None, syscalls_usage: SyscallUsageMap::new() }
    }

    pub fn set_syscall_ptr(&mut self, syscall_ptr: Relocatable) {
        self.syscall_ptr = Some(syscall_ptr);
    }
}
