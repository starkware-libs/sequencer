use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
};
use cairo_vm::types::relocatable::Relocatable;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn set_sha256_segment_in_syscall_handler<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let sha256_ptr = get_ptr_from_var_name(Ids::Sha256Ptr.into(), vm, ids_data, ap_tracking)?;
    hint_processor.syscall_hint_processor.set_sha256_segment(sha256_ptr);
    Ok(())
}

pub(crate) fn log_remaining_txs<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let n_txs = get_integer_from_var_name(Ids::NTxs.into(), vm, ids_data, ap_tracking)?;
    log::debug!("execute_transactions_inner: {n_txs} transactions remaining.");
    Ok(())
}

pub(crate) fn fill_holes_in_rc96_segment<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let rc96_ptr = get_ptr_from_var_name(Ids::RangeCheck96Ptr.into(), vm, ids_data, ap_tracking)?;
    let segment_size = rc96_ptr.offset;
    let base = Relocatable::from((rc96_ptr.segment_index, 0));

    for i in 0..segment_size {
        let address = (base + i)?;
        if vm.get_maybe(&address).is_none() {
            vm.insert_value(address, Felt::ZERO)?;
        }
    }

    Ok(())
}

pub(crate) fn set_component_hashes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn sha2_finalize<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn segments_add_temp<S: StateReader>(
    HintArgs { vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let temp_segment = vm.add_temporary_segment();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::SegmentsAddTemp), temp_segment)
}

pub(crate) fn set_ap_to_actual_fee<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn skip_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn start_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL at the new implementation of
    //   start_tx().
    todo!()
}

pub(crate) fn os_input_transactions<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let num_txns = hint_processor.execution_helper.os_input.transactions.len();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::OsInputTransactions), num_txns)
}

pub(crate) fn segments_add<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
