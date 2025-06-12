use cairo_vm::types::program::Program;
use starknet_types_core::felt::Felt;

use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::error::OsHintError;

pub(crate) trait CommonHintProcessor<'a> {
    // The program being run. The hint processor does not require ownership.
    fn get_program(&self) -> &'a Program;
    fn get_mut_state_update_pointers(&mut self) -> &mut Option<StateUpdatePointers>;
    // KZG fields.
    fn _get_da_segment(&mut self) -> &mut Option<Vec<Felt>>;
    #[allow(clippy::result_large_err)]
    fn set_da_segment(&mut self, da_segment: Vec<Felt>) -> Result<(), OsHintError>;
    // Indicates wether to create pages or not when serializing data-availability.
    fn get_serialize_data_availability_create_pages(&self) -> bool;
    // For testing, track hint coverage.
    fn get_unused_hints(
        &mut self,
    ) -> &mut std::collections::HashSet<crate::hints::enum_definition::AllHints>;
}
