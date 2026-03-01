use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use num_traits::ToPrimitive;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::execute_transactions::utils::{
    calculate_padding,
    N_MISSING_BLOCKS_BOUND,
    SHA256_INPUT_CHUNK_SIZE_BOUND,
};
use crate::hints::types::HintContext;
use crate::hints::vars::{Const, Ids};

pub(crate) fn log_remaining_txs(ctx: HintContext<'_>) -> OsHintResult {
    let n_txs = ctx.get_integer(Ids::NTxs)?;
    log::info!("execute_transactions_inner: {n_txs} transactions remaining.");
    Ok(())
}

pub(crate) fn fill_holes_in_rc96_segment(ctx: HintContext<'_>) -> OsHintResult {
    let rc96_ptr = ctx.get_ptr(Ids::RangeCheck96Ptr)?;
    let segment_size = rc96_ptr.offset;
    let base = Relocatable::from((rc96_ptr.segment_index, 0));

    for i in 0..segment_size {
        let address = (base + i)?;
        if ctx.vm.get_maybe(&address).is_none() {
            ctx.vm.insert_value(address, Felt::ZERO)?;
        }
    }

    Ok(())
}

/// Assigns the class hash of the current transaction to the component hashes var.
/// Assumes the current transaction is of type Declare.
pub(crate) fn set_component_hashes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
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
    let class_component_hashes_base = ctx.vm.gen_arg(&class_component_hashes)?;
    Ok(ctx.insert_value(Ids::ContractClassComponentHashes, class_component_hashes_base)?)
}

pub(crate) fn sha2_finalize(ctx: HintContext<'_>) -> OsHintResult {
    let batch_size = &ctx.fetch_const(Const::ShaBatchSize)?.to_bigint();
    let n = &ctx.get_integer(Ids::N)?.to_bigint();
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
        felt_to_usize(ctx.fetch_const(Const::Sha256InputChunkSize)?)?;
    assert!(
        (0..SHA256_INPUT_CHUNK_SIZE_BOUND).contains(&sha256_input_chunk_size_felts),
        "sha256_input_chunk_size_felts: {sha256_input_chunk_size_felts} is expected to be in the \
         range [0, {SHA256_INPUT_CHUNK_SIZE_BOUND})."
    );
    let padding = calculate_padding(sha256_input_chunk_size_felts, number_of_missing_blocks);

    let sha_ptr_end = ctx.get_ptr(Ids::Sha256PtrEnd)?;
    ctx.vm.load_data(sha_ptr_end, &padding)?;
    Ok(())
}

pub(crate) fn segments_add_temp_initial_txs_range_check_ptr(
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let temp_segment = ctx.vm.add_temporary_segment();
    Ok(ctx.insert_value(Ids::InitialTxsRangeCheckPtr, temp_segment)?)
}

pub(crate) fn load_actual_fee<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let actual_fee = Felt::from(
        hint_processor
            .get_current_execution_helper()?
            .tx_execution_iter
            .get_tx_execution_info_ref()?
            .tx_execution_info
            .actual_fee,
    );
    Ok(ctx.insert_value(Ids::LowActualFee, actual_fee)?)
}

pub(crate) fn skip_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    Ok(hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.skip_tx()?)
}

pub(crate) fn start_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.start_tx()?;
    Ok(())
}

pub(crate) fn os_input_transactions<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let num_txns = hint_processor.get_current_execution_helper()?.os_block_input.transactions.len();
    Ok(ctx.insert_value(Ids::NTxs, num_txns)?)
}

pub(crate) fn segments_add(ctx: HintContext<'_>) -> OsHintResult {
    let segment = ctx.vm.add_memory_segment();
    Ok(insert_value_into_ap(ctx.vm, segment)?)
}
