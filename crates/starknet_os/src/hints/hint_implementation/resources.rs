use blockifier::execution::call_info::CallInfo;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_integer_from_var_name;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;
pub(crate) fn remaining_gas_gt_max<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

// indoc! {r#"
// if execution_helper.debug_mode:
//     expected_initial_gas = execution_helper.call_info.call.initial_gas
//     call_initial_gas = ids.remaining_gas
//     assert expected_initial_gas == call_initial_gas, (
//         f"Expected remaining_gas {expected_initial_gas}. Got: {call_initial_gas}.\n"
//         f"{execution_helper.call_info=}"
//      )"
// #}

pub(crate) fn debug_expected_initial_gas<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    if hint_processor.execution_helper.os_input.debug_mode {
        let call_info: CallInfo = hint_processor.execution_helper._call_info;
        let expected_initial_gas = call_info.call.initial_gas.into();
        let call_initial_gas =
            get_integer_from_var_name(Ids::GasUsage.into(), vm, ids_data, ap_tracking)?;
        if expected_initial_gas != call_initial_gas {
            return Err(OsHintError::ExpectedInitialGas {
                expected_initial_gas,
                call_initial_gas,
                call_info,
            });
        }
    }
    Ok(())
}

pub(crate) fn is_sierra_gas_mode<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
