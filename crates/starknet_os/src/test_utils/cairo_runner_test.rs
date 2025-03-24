use cairo_vm::types::builtin_name::BuiltinName;

use crate::test_utils::cairo_runner::{
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::test_utils::utils::run_cairo_function_and_check_result;

/// from starkware.cairo.common.alloc import alloc
/// from starkware.cairo.common.math import assert_lt
///
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
///     SimpleStruct*, compound_struct: CompoundStruct*) ->
///     (res1: felt, res2: felt*, res3: felt*, res4: CompoundStruct*) {
///     let (res_array: felt*) = alloc();
///     let (res_tuple: felt*) = alloc();
///     let (res_compound_struct: CompoundStruct*) = alloc();
///     assert res_array[0] = array[0] + 1;
///     assert res_array[1] = array[1] + 2;
///     assert res_tuple[0] = tuple[0] + 1;
///     assert res_tuple[1] = tuple[1] + 2;
///     assert res_compound_struct.a = compound_struct.a + 1;
///     assert res_compound_struct.simple_struct = compound_struct.simple_struct;
///     let res_number = number + 1;
///     return (res1=res_number, res2=res_array, res3=res_tuple, res4=res_compound_struct);
/// }
///
/// func pass_structs_and_tuples(tuple: (felt, felt), named_tuple: (a: felt, b:felt),
///     simple_struct: SimpleStruct, compound_struct: CompoundStruct) ->
///     (res1: (a: felt, b:felt), res2: SimpleStruct, res3: CompoundStruct) {
///     alloc_locals;
///     local res_simple_struct: SimpleStruct;
///     local res_compound_struct: CompoundStruct;
///     local res_named_tuple: (a: felt, b: felt);
///     assert res_simple_struct = SimpleStruct(a=simple_struct.a + 1, b=simple_struct.b + 2);
///     assert res_compound_struct = CompoundStruct(a=compound_struct.a + 1,
///     simple_struct=compound_struct.simple_struct);
///     assert res_named_tuple = (a=named_tuple.a + tuple[0], b=named_tuple.b + tuple[1]);
///     return (res1=res_named_tuple, res2=res_simple_struct, res3=res_compound_struct);
/// }
///
/// func pass_implicit_args{range_check_ptr, compound_struct: CompoundStruct*, simple_struct:
///     SimpleStruct}(number_1: felt, number_2: felt) -> (res: felt) {
///     assert_lt(number_1, number_2);
///     let sum = number_1 + number_2;
///     return (res=sum);
/// }
const COMPILED_DUMMY_FUNCTION: &str = include_str!("compiled_dummy_function.json");

#[test]
fn test_felt_and_pointers() -> Cairo0EntryPointRunnerResult<()> {
    // Parameters.
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

    // Expected return values.
    let res1 = EndpointArg::Value(ValueArg::Single((number + 1).into()));
    let res2 = EndpointArg::Pointer(PointerArg::Array(vec![
        (first_array_val + 1).into(),
        (second_array_val + 2).into(),
    ]));
    let res3 = EndpointArg::Pointer(PointerArg::Array(vec![
        (first_tuple_val + 1).into(),
        (second_tuple_val + 2).into(),
    ]));
    let res_simple_struct = EndpointArg::Pointer(PointerArg::Array(vec![
        first_simple_struct_val.into(),
        second_simple_struct_val.into(),
    ]));
    let res4 = EndpointArg::Pointer(PointerArg::Composed(vec![
        (compound_struct_val + 1).into(),
        res_simple_struct.clone(),
    ]));

    run_cairo_function_and_check_result(
        COMPILED_DUMMY_FUNCTION,
        "pass_felt_and_pointers",
        &[number.into(), array, tuple, simple_struct, compound_struct],
        &[],
        &[res1, res2, res3, res4],
        &[],
    )
}

#[test]
fn test_tuples_and_structs() -> Cairo0EntryPointRunnerResult<()> {
    // Parameters.
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

    // Expected return values.
    let res1 = EndpointArg::Value(ValueArg::Array(vec![
        (first_named_tuple_val + first_tuple_val).into(),
        (second_named_tuple_val + second_tuple_val).into(),
    ]));
    let res2 = EndpointArg::Value(ValueArg::Array(vec![
        (first_simple_struct_val + 1).into(),
        (second_simple_struct_val + 2).into(),
    ]));
    let res_simple_struct_pointer = EndpointArg::Pointer(PointerArg::Array(vec![
        first_simple_struct_val.into(),
        second_simple_struct_val.into(),
    ]));
    let res3 = EndpointArg::Value(ValueArg::Composed(vec![
        (compound_struct_val + 1).into(),
        res_simple_struct_pointer,
    ]));

    run_cairo_function_and_check_result(
        COMPILED_DUMMY_FUNCTION,
        "pass_structs_and_tuples",
        &[tuple, named_tuple, simple_struct, compound_struct],
        &[],
        &[res1, res2, res3],
        &[],
    )
}

#[test]
fn test_implicit_args() -> Cairo0EntryPointRunnerResult<()> {
    let number_1 = 1;
    let number_2 = 2;
    let (first_simple_struct_val, second_simple_struct_val) = (7, 8);
    let compound_struct_val = 9;
    let simple_struct = EndpointArg::Value(ValueArg::Array(vec![
        first_simple_struct_val.into(),
        second_simple_struct_val.into(),
    ]));
    let inner_simple_struct = EndpointArg::Pointer(PointerArg::Array(vec![
        first_simple_struct_val.into(),
        (second_simple_struct_val + 1).into(),
    ]));
    let compound_struct = EndpointArg::Pointer(PointerArg::Composed(vec![
        compound_struct_val.into(),
        inner_simple_struct,
    ]));
    run_cairo_function_and_check_result(
        COMPILED_DUMMY_FUNCTION,
        "pass_implicit_args",
        &[number_1.into(), number_2.into()],
        &[
            ImplicitArg::Builtin(BuiltinName::range_check),
            ImplicitArg::NonBuiltin(compound_struct.clone()),
            ImplicitArg::NonBuiltin(simple_struct.clone()),
        ],
        &[(number_1 + number_2).into()],
        &[
            ImplicitArg::Builtin(BuiltinName::range_check),
            ImplicitArg::NonBuiltin(compound_struct),
            ImplicitArg::NonBuiltin(simple_struct),
        ],
    )
}
