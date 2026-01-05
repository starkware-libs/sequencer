use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
};

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::get_address_of_nested_fields;

/// Test hint for debugging. Implement this hint however you like, but should not be merged with
/// an actual implementation.
/// As long as the hint string starts with TEST_HINT_PREFIX (possibly preceded by whitespace),
/// it will be recognized as the test hint and this implementation will be called. For example:
///
/// %{
///     # TEST HINT 7
///     print("Debug hint 7")
/// %}
///
/// The original hint string is passed as the first argument to allow injecting multiple test
/// hints; the implementation can differ depending on the hint string. With the example above, an
/// example implementation could look like:
/// ```ignore
/// pub(crate) fn test_hint<S: StateReader>(
///     hint_str: &str,
///     hint_processor: &mut SnosHintProcessor<'_, S>,
///     HintArgs { .. }: HintArgs<'_>,
/// ) -> OsHintResult {
///     let hint_case = hint_str.trim_start().strip_prefix(TEST_HINT_PREFIX).unwrap().trim_start();
///     match hint_case[0] {
///         '7' => println!("Debug hint 7"),
///         other => panic!("Unknown test hint case {other}."),
///     }
///     Ok(())
/// }
/// ```
pub(crate) fn test_hint<S: StateReader>(
    _hint_str: &str,
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let contract_address =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking).unwrap();
    let response_value =
        get_integer_from_var_name(Ids::Value.into(), vm, ids_data, ap_tracking).unwrap();
    // let response_value = vm
    //     .get_integer(
    //         get_address_of_nested_fields(
    //             ids_data,
    //             Ids::SyscallPtr,
    //             CairoStruct::StorageReadPtr,
    //             vm,
    //             ap_tracking,
    //             &["response", "value"],
    //             hint_processor.program,
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    // let request_key = vm
    //     .get_integer(
    //         get_address_of_nested_fields(
    //             ids_data,
    //             Ids::Request,
    //             CairoStruct::StorageReadRequestPtr,
    //             vm,
    //             ap_tracking,
    //             &["key"],
    //             hint_processor.program,
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    let request_key = vm
        .get_integer(
            (get_ptr_from_var_name(Ids::Request.into(), vm, ids_data, ap_tracking).unwrap()
                + 1usize)
                .unwrap(),
        )
        .unwrap();
    let request_value = hint_processor
        .get_current_execution_helper()
        .unwrap()
        .cached_state
        .get_storage_at(contract_address.try_into().unwrap(), (*request_key).try_into().unwrap())
        .unwrap();
    tracing::warn!(
        "In execute_storage_read: contract address: {contract_address:?}, request key: \
         {request_key:?}, request value: {request_value:?}, response value: {response_value:?}"
    );
    Ok(())
}

/// Same as [test_hint], but for the aggregator program.
pub(crate) fn test_aggregator_hint(
    _hint_str: &str,
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    Ok(())
}
