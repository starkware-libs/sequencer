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
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[ImplicitArg],
) -> Cairo0EntryPointRunnerResult<()> {
    let (actual_implicit_retdata, actual_explicit_retdata) = run_cairo_0_entry_point(
        program_str,
        function_name,
        explicit_args,
        implicit_args,
        expected_explicit_retdata,
    )?;
    assert_eq!(expected_explicit_retdata, &actual_explicit_retdata);
    assert_eq!(expected_implicit_retdata, &actual_implicit_retdata);
    Ok(())
}
