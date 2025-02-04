pub mod errors;
pub mod hint_processor;
pub mod hints;
pub mod io;

use blockifier::context::BlockContext;
use blockifier::state::state_api::StateReader;
use cairo_vm::types::layout_name::LayoutName;

use crate::errors::StarknetOsError;
use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::io::os_input::StarknetOsInput;
use crate::io::os_output::StarknetOsRunnerOutput;

pub fn run_os<T: StateReader>(
    _compiled_os: &[u8],
    _layout: LayoutName,
    _block_context: BlockContext,
    os_input: &StarknetOsInput,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    let _execution_helper = OsExecutionHelper::<T>::new(os_input);
    todo!()
}
