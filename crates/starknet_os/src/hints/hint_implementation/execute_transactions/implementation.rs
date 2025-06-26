use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use num_traits::ToPrimitive;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::enum_definition::{AllHints, OsHint, StatelessHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::execute_transactions::utils::{
    calculate_padding,
    N_MISSING_BLOCKS_BOUND,
    SHA256_INPUT_CHUNK_SIZE_BOUND,
};
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

pub(crate) fn set_sha256_segment_in_syscall_handler<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let sha256_ptr = get_ptr_from_var_name(Ids::Sha256Ptr.into(), vm, ids_data, ap_tracking)?;
    hint_processor.get_mut_current_execution_helper()?.syscall_hint_processor.sha256_segment =
        Some(sha256_ptr);
    Ok(())
}

pub(crate) fn log_remaining_txs(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let n_txs = get_integer_from_var_name(Ids::NTxs.into(), vm, ids_data, ap_tracking)?;
    log::debug!("execute_transactions_inner: {n_txs} transactions remaining.");
    Ok(())
}

pub(crate) fn fill_holes_in_rc96_segment(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
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
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
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
    let class_component_hashes: Vec<_> = component_hashes
        .get(&class_hash)
        .ok_or_else(|| OsHintError::MissingComponentHashes(class_hash))?
        .flatten()
        .into_iter()
        .map(MaybeRelocatable::from)
        .collect();
    let class_component_hashes_base = vm.gen_arg(&class_component_hashes)?;
    Ok(insert_value_from_var_name(
        Ids::ContractClassComponentHashes.into(),
        class_component_hashes_base,
        vm,
        ids_data,
        ap_tracking,
    )?)
}

pub(crate) fn sha2_finalize(
    HintArgs { constants, ids_data, ap_tracking, vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let batch_size = &Const::ShaBatchSize.fetch(constants)?.to_bigint();
    let n = &get_integer_from_var_name(Ids::N.into(), vm, ids_data, ap_tracking)?.to_bigint();
    // Calculate the modulus operation, not the remainder.
    let number_of_missing_blocks = ((((-n) % batch_size) + batch_size) % batch_size)
        .to_u32()
        .expect("Failed to convert number of missing blocks to u32.");
    assert!(
        (0..N_MISSING_BLOCKS_BOUND).contains(&number_of_missing_blocks),
        "number_of_missing_blocks: {number_of_missing_blocks} is expected to be in the range [0, \
         {N_MISSING_BLOCKS_BOUND}). Got n: {n} and batch size: {batch_size}."
    );
    let sha256_input_chunk_size_felts =
        felt_to_usize(Const::Sha256InputChunkSize.fetch(constants)?)?;
    assert!(
        (0..SHA256_INPUT_CHUNK_SIZE_BOUND).contains(&sha256_input_chunk_size_felts),
        "sha256_input_chunk_size_felts: {sha256_input_chunk_size_felts} is expected to be in the \
         range [0, {SHA256_INPUT_CHUNK_SIZE_BOUND})."
    );
    let padding = calculate_padding(sha256_input_chunk_size_felts, number_of_missing_blocks);

    let sha_ptr_end = get_ptr_from_var_name(Ids::Sha256PtrEnd.into(), vm, ids_data, ap_tracking)?;
    vm.load_data(sha_ptr_end, &padding)?;
    Ok(())
}

pub(crate) fn segments_add_temp(HintArgs { vm, .. }: HintArgs<'_>) -> OsHintResult {
    let temp_segment = vm.add_temporary_segment();
    insert_nondet_hint_value(
        vm,
        AllHints::StatelessHint(StatelessHint::SegmentsAddTemp),
        temp_segment,
    )
}

pub(crate) fn set_ap_to_actual_fee<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let actual_fee = hint_processor
        .get_current_execution_helper()?
        .tx_execution_iter
        .get_tx_execution_info_ref()?
        .tx_execution_info
        .actual_fee;
    insert_value_into_ap(vm, Felt::from(actual_fee))?;
    Ok(())
}

pub(crate) fn skip_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    Ok(hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.skip_tx()?)
}

pub(crate) fn start_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    let tx_type = hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?.tx_type();
    hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.start_tx(tx_type)?;
    Ok(())
}

pub(crate) fn os_input_transactions<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let num_txns = hint_processor.get_current_execution_helper()?.os_block_input.transactions.len();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::OsInputTransactions), num_txns)
}

pub(crate) fn segments_add(HintArgs { vm, .. }: HintArgs<'_>) -> OsHintResult {
    let segment = vm.add_memory_segment();
    Ok(insert_value_into_ap(vm, segment)?)
}
