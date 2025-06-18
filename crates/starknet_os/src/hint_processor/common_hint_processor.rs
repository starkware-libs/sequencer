use cairo_vm::hint_processor::hint_processor_definition::HintExtension;
use cairo_vm::types::program::Program;
use cairo_vm::vm::errors::hint_errors::HintError as VmHintError;
use starknet_types_core::felt::Felt;

use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::error::OsHintError;

pub(crate) type VmHintResultType<T> = Result<T, VmHintError>;
pub(crate) type VmHintResult = VmHintResultType<()>;
pub(crate) type VmHintExtensionResult = VmHintResultType<HintExtension>;

pub(crate) trait CommonHintProcessor<'a> {
    // The program being run. The hint processor does not require ownership.
    fn get_program(&self) -> &'a Program;
    fn get_mut_state_update_pointers(&mut self) -> &mut Option<StateUpdatePointers>;
    // KZG fields.
    fn get_da_segment(&mut self) -> &mut Option<Vec<Felt>>;
    #[allow(clippy::result_large_err)]
    fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> Result<(), OsHintError>;
    // Indicates wether to create pages or not when serializing data-availability.
    fn get_serialize_data_availability_create_pages(&self) -> bool;
    // For testing, track hint coverage.
    #[cfg(any(test, feature = "testing"))]
    fn get_unused_hints(
        &mut self,
    ) -> &mut std::collections::HashSet<crate::hints::enum_definition::AllHints>;
}

#[macro_export]
macro_rules! impl_common_hint_processor {
    ($hint_processor:ident $(, $generic_var:ident, $generic:ident)?) => {
        impl<'program $(, $generic_var: $generic )?> CommonHintProcessor<'program>
            for $hint_processor<'program, $($generic_var)?>
        {
            fn get_program(&self) -> &'program Program {
                self.program
            }

            fn get_mut_state_update_pointers(&mut self) -> &mut Option<StateUpdatePointers> {
                &mut self.state_update_pointers
            }

            fn get_da_segment(&mut self) -> &mut Option<Vec<Felt>> {
                &mut self.da_segment
            }

            fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> Result<(), OsHintError> {
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

            #[cfg(any(test, feature = "testing"))]
            fn get_unused_hints(&mut self) -> &mut std::collections::HashSet<AllHints> {
                &mut self.unused_hints
            }
        }
    };
}
