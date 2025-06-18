use cairo_lang_casm::hints::Hint as Cairo1Hint;
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
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

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
use crate::impl_common_hint_processor;

pub struct AggregatorHintProcessor<'a> {
    // The program being run. The hint processor does not require ownership.
    pub(crate) program: &'a Program,
    pub(crate) state_update_pointers: Option<StateUpdatePointers>,
    // KZG fields.
    da_segment: Option<Vec<Felt>>,
    // Indicates wether to create pages or not when serializing data-availability.
    pub(crate) serialize_data_availability_create_pages: bool,
    builtin_hint_processor: BuiltinHintProcessor,
    // For testing, track hint coverage.
    #[cfg(any(test, feature = "testing"))]
    pub unused_hints: std::collections::HashSet<AllHints>,
}

impl_common_hint_processor!(AggregatorHintProcessor);

impl HintProcessorLogic for AggregatorHintProcessor<'_> {
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
            let hint_str = hint_processor_data.code.as_str();
            if let Ok(hint) = AllHints::from_str(hint_str) {
                // Aggregator hint, Cairo0 syscall.
                return match hint {
                    AllHints::StatelessHint(stateless) => {
                        stateless.execute_hint(self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                    AllHints::CommonHint(common_hint) => {
                        common_hint.execute_hint(self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                    AllHints::AggregatorHint(aggregator_hint) => {
                        aggregator_hint.execute_hint(self, hint_args)?;
                        Ok(HintExtension::default())
                    }
                    AllHints::OsHint(_)
                    | AllHints::DeprecatedSyscallHint(_)
                    | AllHints::HintExtension(_) => {
                        panic!("Aggregator received OS hints");
                    }
                    #[cfg(any(test, feature = "testing"))]
                    AllHints::TestHint => {
                        test_aggregator_hint(hint_processor_data.code.as_str(), self, hint_args)?;
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
                let no_temporary_segments = false;
                execute_core_hint_base(vm, exec_scopes, hint, no_temporary_segments)?;
                Ok(HintExtension::default())
            }
            Cairo1Hint::Starknet(hint) => {
                panic!("Aggregator should not accept starknet hints: {hint:?}");
            }
            Cairo1Hint::External(_) => {
                panic!("starknet should never accept classes with external hints!")
            }
        }
    }
}
