use blockifier::retdata;

use crate::test_utils::cairo_runner::{EndpointArg, PointerArg, ValueArg};
use crate::test_utils::errors::Cairo0EntryPointRunnerError;
use crate::test_utils::utils::run_cairo_function_and_check_result;

/// struct CompoundStruct {
///     a: felt,
///     simple_struct: SimpleStruct*,
/// }
///
/// struct SimpleStruct {
///     a: felt,
///     b: felt,
/// }
///
/// func pass_felt_and_pointers(number: felt, array: felt*, tuple: felt*, simple_struct:
///     SimpleStruct*, compound_struct: CompoundStruct*) -> (res1: felt, res2: felt, res3: felt) {
///     let res1 = number + array[0];
///     let res2 = tuple[0] + tuple[1];
///     let res3 = simple_struct.a + compound_struct.simple_struct.b;
///     return (res1=res1, res2=res2, res3=res3);
/// }
///
/// func pass_structs_and_tuples(tuple: (felt, felt), named_tuple: (a: felt, b:felt), simple_struct:
///     SimpleStruct, compound_struct: CompoundStruct) -> (res1: felt, res2: felt, res3: felt) {
///     let res1 = tuple[0] + tuple[1];
///     let res2 = named_tuple.a + named_tuple.b;
///     let res3 = simple_struct.a + compound_struct.simple_struct.b;
///     return (res1=res1, res2=res2, res3=res3);
/// }
const COMPILED_DUMMY_FUNCTION: &str = include_str!("compiled_dummy_function.json");

#[test]
fn test_felt_and_pointers() -> Result<(), Cairo0EntryPointRunnerError> {
    let number = 2;
    let (first_array_val, second_array_val) = (3, 4);
    let (first_tuple_val, second_tuple_val) = (5, 6);
    let (first_simple_struct_val, second_simple_struct_val) = (7, 8);
    let compound_struct_val = 9;
    let array = EndpointArg::Pointer(PointerArg::Array(vec![
        first_array_val.into(),
        second_array_val.into(),
    ]));
    let tuple = EndpointArg::Pointer(PointerArg::Array(vec![
        first_tuple_val.into(),
        second_tuple_val.into(),
    ]));
    let simple_struct = EndpointArg::Pointer(PointerArg::Array(vec![
        first_simple_struct_val.into(),
        second_simple_struct_val.into(),
    ]));
    let compound_struct = EndpointArg::Pointer(PointerArg::Composed(vec![
        compound_struct_val.into(),
        simple_struct.clone(),
    ]));
    run_cairo_function_and_check_result(
        COMPILED_DUMMY_FUNCTION,
        "pass_felt_and_pointers",
        &[number.into(), array, tuple, simple_struct, compound_struct],
        &retdata![
            (number + first_array_val).into(),
            (first_tuple_val + second_tuple_val).into(),
            (first_simple_struct_val + second_simple_struct_val).into()
        ],
    )
}

#[test]
fn test_tuples_and_structs() -> Result<(), Cairo0EntryPointRunnerError> {
    let (first_tuple_val, second_tuple_val) = (3, 4);
    let (first_named_tuple_val, second_named_tuple_val) = (5, 6);
    let (first_simple_struct_val, second_simple_struct_val) = (7, 8);
    let compound_struct_val = 9;
    let tuple =
        EndpointArg::Value(ValueArg::Array(vec![first_tuple_val.into(), second_tuple_val.into()]));
    let named_tuple = EndpointArg::Value(ValueArg::Array(vec![
        first_named_tuple_val.into(),
        second_named_tuple_val.into(),
    ]));
    let simple_struct = EndpointArg::Value(ValueArg::Array(vec![
        first_simple_struct_val.into(),
        second_simple_struct_val.into(),
    ]));
    let simple_struct_pointer = EndpointArg::Pointer(PointerArg::Array(vec![
        first_simple_struct_val.into(),
        second_simple_struct_val.into(),
    ]));
    let compound_struct = EndpointArg::Value(ValueArg::Composed(vec![
        compound_struct_val.into(),
        simple_struct_pointer,
    ]));
    run_cairo_function_and_check_result(
        COMPILED_DUMMY_FUNCTION,
        "pass_structs_and_tuples",
        &[tuple, named_tuple, simple_struct, compound_struct],
        &retdata![
            (first_tuple_val + second_tuple_val).into(),
            (first_named_tuple_val + second_named_tuple_val).into(),
            (first_simple_struct_val + second_simple_struct_val).into()
        ],
    )
}
