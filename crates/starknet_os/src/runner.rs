use blockifier::context::BlockContext;
use blockifier::state::state_api::StateReader;
use cairo_vm::types::layout_name::LayoutName;

use crate::errors::StarknetOsError;
use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hint_processor::panicking_state_reader::PanickingStateReader;
use crate::io::os_input::StarknetOsInput;
use crate::io::os_output::StarknetOsRunnerOutput;

pub fn run_os<S: StateReader>(
    _compiled_os: &[u8],
    _layout: LayoutName,
    _block_context: BlockContext,
    os_input: &StarknetOsInput,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    let _execution_helper = OsExecutionHelper::<S>::new(os_input);
    todo!()
}

/// Run the OS with a "stateless" state reader - panics if accesses state that is not pre-loaded
/// from the OS input.
pub fn run_os_stateless(
    compiled_os: &[u8],
    layout: LayoutName,
    block_context: BlockContext,
    os_input: &StarknetOsInput,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    run_os::<PanickingStateReader>(compiled_os, layout, block_context, os_input)
}
