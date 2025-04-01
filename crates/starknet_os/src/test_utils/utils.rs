use std::any::Any;
use std::collections::HashMap;

use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

pub fn run_cairo_function_and_check_result(
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> Cairo0EntryPointRunnerResult<()> {
    let (actual_implicit_retdata, actual_explicit_retdata) = run_cairo_0_entry_point(
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

pub fn create_squashed_cairo_dict(
    prev_values: &HashMap<Felt, EndpointArg>,
    new_values: &HashMap<Felt, EndpointArg>,
) -> EndpointArg {
    let mut squashed_dict: Vec<EndpointArg> = vec![];
    for (key, value) in new_values.iter() {
        let prev_value: &EndpointArg =
            prev_values.get(key).unwrap_or(&EndpointArg::Value(ValueArg::Single(Felt::ZERO)));
        squashed_dict.push((*key).into());
        squashed_dict.push(prev_value.clone());
        squashed_dict.push(value.clone());
    }
    EndpointArg::Pointer(PointerArg::Composed(squashed_dict))
}
