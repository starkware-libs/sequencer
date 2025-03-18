use blockifier::execution::call_info::Retdata;
use cairo_vm::types::program::Program;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::test_utils::cairo_runner::{run_cairo_0_entry_point, EndpointArg};
use crate::test_utils::errors::Cairo0EntryPointRunnerError;

pub fn run_cairo_function_and_check_result(
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    expected_retdata: &Retdata,
) -> Result<(), Cairo0EntryPointRunnerError> {
    let program_bytes = program_str.as_bytes();
    let program = Program::from_bytes(program_bytes, None).unwrap();
    let (state_reader, os_input) = (None, None);
    let hint_processor =
        SnosHintProcessor::new_for_testing(state_reader, os_input, Some(program.clone()));
    let actual_retdata = run_cairo_0_entry_point(
        &program,
        function_name,
        expected_retdata.0.len(),
        explicit_args,
        hint_processor,
    )?;
    assert_eq!(expected_retdata, &actual_retdata);
    Ok(())
}
