use cairo_lang_casm::hints::StarknetHint;
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::BuiltinHintProcessor;
use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::types::program::Program;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::enum_definition::AllHints;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

pub(crate) type VmHintResultType<T> = Result<T, VmHintError>;
pub(crate) type VmHintResult = VmHintResultType<()>;
pub(crate) type VmHintExtensionResult = VmHintResultType<HintExtension>;

pub(crate) trait CommonHintProcessor<'a> {
    // The program being run. The hint processor does not require ownership.
    fn get_program(&self) -> &'a Program;
    fn get_mut_state_update_pointers(&mut self) -> &mut Option<StateUpdatePointers>;
    // KZG fields.
    fn get_da_segment(&mut self) -> &mut Option<Vec<Felt>>;
    fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> OsHintResult;
    // Indicates wether to create pages or not when serializing data-availability.
    fn get_serialize_data_availability_create_pages(&self) -> bool;
    fn get_builtin_hint_processor(&mut self) -> &mut BuiltinHintProcessor;
    // For testing, track hint coverage.
    #[cfg(any(test, feature = "testing"))]
    fn get_unused_hints(
        &mut self,
    ) -> &mut std::collections::HashSet<crate::hints::enum_definition::AllHints>;

    fn execute_cairo0_unique_hint(
        &mut self,
        hint: &AllHints,
        hint_args: HintArgs<'_>,
        _hint_str: &str,
    ) -> VmHintExtensionResult;

    fn execute_cairo1_unique_hint(
        &mut self,
        hint: &StarknetHint,
        vm: &mut VirtualMachine,
    ) -> VmHintExtensionResult;
}

#[macro_export]
macro_rules! impl_common_hint_processor_logic {
    () => {
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
                    // OS hint, Cairo0 syscall.
                    return match hint {
                        AllHints::StatelessHint(stateless) => {
                            stateless.execute_hint(self, hint_args)?;
                            Ok(HintExtension::default())
                        }
                        AllHints::CommonHint(common_hint) => {
                            common_hint.execute_hint(self, hint_args)?;
                            Ok(HintExtension::default())
                        }
                        _ => self.execute_cairo0_unique_hint(&hint, hint_args, hint_str),
                    };
                } else {
                    // Cairo0 core hint.
                    self.get_builtin_hint_processor().execute_hint(
                        vm,
                        exec_scopes,
                        hint_data,
                        constants,
                    )?;
                    return Ok(HintExtension::default());
                }
            }

            // Cairo1 syscall or Cairo1 core hint.
            match hint_data.downcast_ref::<Cairo1Hint>().ok_or(VmHintError::WrongHintData)? {
                Cairo1Hint::Core(hint) => {
                    let no_temporary_segments = false;
                    execute_core_hint_base(vm, exec_scopes, &hint, no_temporary_segments)?;
                    Ok(HintExtension::default())
                }
                Cairo1Hint::Starknet(hint) => self.execute_cairo1_unique_hint(hint, vm),
                Cairo1Hint::External(_) => {
                    panic!("starknet should never accept classes with external hints!")
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_common_hint_processor_getters {
    () => {
        fn get_program(&self) -> &'program Program {
            self.program
        }

        fn get_mut_state_update_pointers(&mut self) -> &mut Option<StateUpdatePointers> {
            &mut self.state_update_pointers
        }

        fn get_da_segment(&mut self) -> &mut Option<Vec<Felt>> {
            &mut self.da_segment
        }

        fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> OsHintResult {
            if self.da_segment.is_some() {
                return Err(OsHintError::AssertionFailed {
                    message: "DA segment is already initialized.".to_string(),
                });
            }
            self.da_segment = Some(da_segment);
            Ok(())
        }

        fn get_serialize_data_availability_create_pages(&self) -> bool {
            self.serialize_data_availability_create_pages
        }

        fn get_builtin_hint_processor(&mut self) -> &mut BuiltinHintProcessor {
            &mut self.builtin_hint_processor
        }

        #[cfg(any(test, feature = "testing"))]
        fn get_unused_hints(&mut self) -> &mut std::collections::HashSet<AllHints> {
            &mut self.unused_hints
        }
    };
}
