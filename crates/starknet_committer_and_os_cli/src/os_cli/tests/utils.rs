use std::any::Any;
use std::collections::HashMap;

use starknet_os::test_utils::cairo_runner::{EndpointArg, ImplicitArg};
use starknet_os::test_utils::utils::run_cairo_function_and_check_result;

use crate::os_cli::tests::types::{OsPythonTestResult, OsSpecificTestError};
use crate::shared_utils::types::PythonTestError;

pub(crate) fn test_cairo_function(
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> OsPythonTestResult {
    run_cairo_function_and_check_result(
        program_str,
        function_name,
        explicit_args,
        implicit_args,
        expected_explicit_retdata,
        expected_implicit_retdata,
        hint_locals,
    )
    .map_err(|error| {
        PythonTestError::SpecificError(OsSpecificTestError::Cairo0EntryPointRunner(error))
    })?;
    Ok("".to_string())
}
