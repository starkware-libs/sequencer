pub mod errors;
pub mod hint_processor;
pub mod hints;

use blockifier::context::BlockContext;
use cairo_vm::types::layout_name::LayoutName;
use errors::StarknetOsError;
use hint_processor::execution_helper::OsExecutionHelper;
use hint_processor::os_output::StarknetOsRunnerOutput;
use hint_processor::os_state_reader::OsStateReader;

pub fn run_os<T: OsStateReader>(
    _compiled_os: &[u8],
    _layout: LayoutName,
    _block_context: BlockContext,
    _execution_helper: OsExecutionHelper<T>,
) -> Result<StarknetOsRunnerOutput, StarknetOsError> {
    todo!()
}
