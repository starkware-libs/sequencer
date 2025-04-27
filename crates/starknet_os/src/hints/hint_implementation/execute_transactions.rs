use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::types::relocatable::Relocatable;
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper;
use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn set_sha256_segment_in_syscall_handler<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let sha256_ptr = get_ptr_from_var_name(Ids::Sha256Ptr.into(), vm, ids_data, ap_tracking)?;
    hint_processor.syscall_hint_processor.set_sha256_segment(sha256_ptr);
    Ok(())
}

pub(crate) fn log_remaining_txs<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let n_txs = get_integer_from_var_name(Ids::NTxs.into(), vm, ids_data, ap_tracking)?;
    log::debug!("execute_transactions_inner: {n_txs} transactions remaining.");
    Ok(())
}

pub(crate) fn fill_holes_in_rc96_segment<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
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

/// Assigns the class hash of the current transaction to the component hashes var.
/// Assumes the current transaction is of type Declare.
pub(crate) fn set_component_hashes<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let current_execution_helper = hint_processor.get_current_execution_helper()?;
    let account_tx = current_execution_helper.tx_tracker.get_account_tx()?;
    let class_hash = if let AccountTransaction::Declare(declare_tx) = account_tx {
        declare_tx.class_hash()
    } else {
        return Err(OsHintError::UnexpectedTxType(account_tx.tx_type()));
    };
    let component_hashes =
        &current_execution_helper.os_block_input.declared_class_hash_to_component_hashes;
    let class_component_hashes = vm.gen_arg(
        component_hashes
            .get(&class_hash)
            .ok_or_else(|| OsHintError::MissingComponentHashes(class_hash))?,
    )?;
    Ok(insert_value_from_var_name(
        Ids::ContractClassComponentHashes.into(),
        class_component_hashes,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn sha2_finalize<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn segments_add_temp<S: StateReader>(
    HintArgs { vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let temp_segment = vm.add_temporary_segment();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::SegmentsAddTemp), temp_segment)
}

pub(crate) fn set_ap_to_actual_fee<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn skip_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn start_tx<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let execution_helper = hint_processor.get_mut_current_execution_helper()?;
    let tx_execution_iter = &mut execution_helper.tx_execution_iter;
    if tx_execution_iter.tx_execution_info_ref.is_some() {
        return Err(OsHintError::AssertionFailed {
            message: "start_tx() called twice in a row".to_string(),
        });
    }

    let tx_type = execution_helper.tx_tracker.get_tx()?.tx_type();
    tx_execution_iter
        .next_tx(tx_type)
        .ok_or_else(|| OsHintError::EndOfIterator { item_type: "tx_execution_info".into() })
}

pub(crate) fn os_input_transactions<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let num_txns = hint_processor.get_current_execution_helper()?.os_block_input.transactions.len();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::OsInputTransactions), num_txns)
}

pub(crate) fn segments_add<S: StateReader>(
    HintArgs { vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let segment = vm.add_memory_segment();
    Ok(insert_value_into_ap(vm, segment)?)
}
