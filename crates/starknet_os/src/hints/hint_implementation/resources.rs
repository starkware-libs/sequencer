use blockifier::execution::contract_class::TrackedResource;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn remaining_gas_gt_max(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let remaining_gas =
        get_integer_from_var_name(Ids::RemainingGas.into(), vm, ids_data, ap_tracking)?;
    let max_gas = get_integer_from_var_name(Ids::MaxGas.into(), vm, ids_data, ap_tracking)?;
    let remaining_gas_gt_max: Felt = (remaining_gas > max_gas).into();
    Ok(insert_value_into_ap(vm, remaining_gas_gt_max)?)
}

pub(crate) fn debug_expected_initial_gas<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let current_execution_helper =
        hint_processor.execution_helpers_manager.get_current_execution_helper()?;
    if current_execution_helper.os_logger.debug {
        let call_info = current_execution_helper
            .tx_execution_iter
            .get_tx_execution_info_ref()?
            .get_call_info_tracker()?
            .call_info;
        let expected_initial_gas = Felt::from(call_info.call.initial_gas);
        let call_initial_gas =
            get_integer_from_var_name(Ids::InnerRemainingGas.into(), vm, ids_data, ap_tracking)?;
        if expected_initial_gas != call_initial_gas {
            return Err(OsHintError::AssertionFailed {
                message: format!(
                    "Expected remaining_gas {expected_initial_gas}. Got: {call_initial_gas}. call \
                     info: {call_info:?}.",
                ),
            });
        }
    }
    Ok(())
}

pub(crate) fn is_sierra_gas_mode<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let gas_mode = hint_processor.get_current_call_info_tracker()?.call_info.tracked_resource;

    Ok(insert_value_from_var_name(
        Ids::IsSierraGasMode.into(),
        Felt::from(gas_mode == TrackedResource::SierraGas),
        vm,
        ids_data,
        ap_tracking,
    )?)
}
