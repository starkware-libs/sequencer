use std::any::Any;
use std::collections::HashMap;

use starknet_os::test_utils::cairo_runner::{EndpointArg, ImplicitArg};
use starknet_os::test_utils::utils::run_cairo_function_and_check_result;

use crate::os_cli::tests::types::{OsPythonTestResult, OsSpecificTestError};
use crate::shared_utils::types::PythonTestError;

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $value: expr ),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut map = HashMap::new();
        $(
            map.insert($key, $value);
        )*
        map
    }};
}

#[macro_export]
macro_rules! felt_to_felt_hashmap {
    ($( $key: expr => $value: expr ),* $(,)?) => {{
        hashmap! {
            $(
                starknet_types_core::felt::Felt::from($key) =>
                starknet_types_core::felt::Felt::from($value),
            )*
        }
    }};
}

#[macro_export]
macro_rules! felt_to_value_hashmap {
    ($( $key: expr => $value: expr ),* $(,)?) => {{
        hashmap! {
            $(
                starknet_types_core::felt::Felt::from($key) =>
                $value,
            )*
        }
    }};
}

#[macro_export]
macro_rules! felt_tuple {
    ($($value: expr),* $(,)?) => {
        (
            $(
                starknet_types_core::felt::Felt::from($value),
            )*
        )
    }
}

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
