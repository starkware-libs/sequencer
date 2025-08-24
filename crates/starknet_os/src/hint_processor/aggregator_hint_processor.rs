use std::path::PathBuf;

use cairo_lang_casm::hints::{Hint as Cairo1Hint, StarknetHint};
use cairo_lang_runner::casm_run::execute_core_hint_base;
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
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::runners::cairo_runner::ResourceTracker;
use cairo_vm::vm::vm_core::VirtualMachine;
use serde::Deserialize;
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

use crate::hint_processor::common_hint_processor::{
    CommonHintProcessor,
    VmHintExtensionResult,
    VmHintResult,
};
use crate::hint_processor::state_update_pointers::StateUpdatePointers;
#[cfg(any(test, feature = "testing"))]
use crate::hint_processor::test_hint::test_aggregator_hint;
use crate::hints::enum_definition::AllHints;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::{HintArgs, HintEnum};
use crate::{impl_common_hint_processor_getters, impl_common_hint_processor_logic};

#[derive(Deserialize, Debug, Clone)]
pub enum DataAvailability {
    Blob(PathBuf),
    CallData,
}

#[derive(Deserialize, Debug)]
pub struct AggregatorInput {
    pub bootloader_output: Vec<Felt>,
    pub full_output: bool,
    pub da: DataAvailability,
    pub debug_mode: bool,
    pub fee_token_address: Felt,
    pub chain_id: Felt,
    pub public_keys: Vec<Felt>,
}

impl AggregatorInput {
    pub fn log_level(&self) -> LevelFilter {
        if self.debug_mode { LevelFilter::DEBUG } else { LevelFilter::INFO }
    }
}

pub struct AggregatorHintProcessor<'a> {
    // The program being run. The hint processor does not require ownership.
    pub(crate) program: &'a Program,
    pub(crate) state_update_pointers: Option<StateUpdatePointers>,
    // KZG fields.
    da_segment: Option<Vec<Felt>>,
    pub(crate) input: AggregatorInput,
    // Indicates wether to create pages or not when serializing data-availability.
    pub(crate) serialize_data_availability_create_pages: bool,
    builtin_hint_processor: BuiltinHintProcessor,
    // For testing, track hint coverage.
    #[cfg(any(test, feature = "testing"))]
    pub unused_hints: std::collections::HashSet<AllHints>,
}

impl<'a> AggregatorHintProcessor<'a> {
    pub fn new(program: &'a Program, input: AggregatorInput) -> Self {
        Self {
            program,
            state_update_pointers: None,
            da_segment: None,
            input,
            serialize_data_availability_create_pages: false,
            builtin_hint_processor: BuiltinHintProcessor::new_empty(),
            #[cfg(any(test, feature = "testing"))]
            unused_hints: AllHints::all_iter().collect(),
        }
    }
}

/// Default implementation (required for the VM to use the type as a hint processor).
impl ResourceTracker for AggregatorHintProcessor<'_> {}

impl HintProcessorLogic for AggregatorHintProcessor<'_> {
    impl_common_hint_processor_logic!();
}

impl<'program> CommonHintProcessor<'program> for AggregatorHintProcessor<'program> {
    impl_common_hint_processor_getters!();

    fn execute_cairo0_unique_hint(
        &mut self,
        hint: &AllHints,
        hint_args: HintArgs<'_>,
        _hint_str: &str,
    ) -> VmHintExtensionResult {
        match hint {
            AllHints::StatelessHint(_) | AllHints::CommonHint(_) => {
                unreachable!(
                    "Stateless and common hints should be handled in execute_hint_extensive \
                     function; got {hint:?}."
                );
            }
            AllHints::AggregatorHint(aggregator_hint) => {
                aggregator_hint.execute_hint(self, hint_args)?;
            }
            AllHints::OsHint(_)
            | AllHints::DeprecatedSyscallHint(_)
            | AllHints::HintExtension(_) => {
                panic!("Aggregator received OS hint: {hint:?}");
            }
            #[cfg(any(test, feature = "testing"))]
            AllHints::TestHint => {
                test_aggregator_hint(_hint_str, self, hint_args)?;
            }
        }
        Ok(HintExtension::default())
    }

    fn execute_cairo1_unique_hint(
        &mut self,
        hint: &StarknetHint,
        _vm: &mut VirtualMachine,
    ) -> VmHintExtensionResult {
        panic!("Aggregator should not accept starknet hints: {hint:?}");
    }
}
