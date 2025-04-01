use std::any::Any;
use std::collections::HashMap;

use crate::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
};

#[allow(clippy::too_many_arguments)]
pub fn run_cairo_function_and_check_result(
    runner_config: Option<EntryPointRunnerConfig>,
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> Cairo0EntryPointRunnerResult<()> {
    let (actual_implicit_retdata, actual_explicit_retdata) = run_cairo_0_entry_point(
        &runner_config.unwrap_or_default(),
        program_str,
        function_name,
        explicit_args,
        implicit_args,
        expected_explicit_retdata,
        hint_locals,
    )?;
    assert_eq!(expected_explicit_retdata, &actual_explicit_retdata);
    assert_eq!(expected_implicit_retdata, &actual_implicit_retdata);
    Ok(())
}
