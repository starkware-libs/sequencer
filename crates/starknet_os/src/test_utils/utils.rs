use blockifier::execution::call_info::Retdata;

use crate::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    ImplicitArg,
};

pub fn run_cairo_function_and_check_result(
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_retdata: &Retdata,
) -> Cairo0EntryPointRunnerResult<()> {
    let actual_retdata = run_cairo_0_entry_point(
        program_str,
        function_name,
        expected_retdata.0.len(),
        explicit_args,
        implicit_args,
    )?;
    assert_eq!(expected_retdata, &actual_retdata);
    Ok(())
}
