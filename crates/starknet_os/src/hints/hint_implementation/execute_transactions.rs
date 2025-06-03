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

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

// TODO(Nimrod): Use the IV defined in the VM once it's public.
const IV: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

#[allow(clippy::result_large_err)]
pub(crate) fn set_sha256_segment_in_syscall_handler<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let sha256_ptr = get_ptr_from_var_name(Ids::Sha256Ptr.into(), vm, ids_data, ap_tracking)?;
    hint_processor.syscall_hint_processor.set_sha256_segment(sha256_ptr);
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn log_remaining_txs<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let n_txs = get_integer_from_var_name(Ids::NTxs.into(), vm, ids_data, ap_tracking)?;
    log::debug!("execute_transactions_inner: {n_txs} transactions remaining.");
    Ok(())
}

#[allow(clippy::result_large_err)]
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
#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn sha2_finalize<S: StateReader>(
    HintArgs { constants, ids_data, ap_tracking, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let batch_size = &Const::ShaBatchSize.fetch(constants)?.to_bigint();
    let n = &get_integer_from_var_name(Ids::N.into(), vm, ids_data, ap_tracking)?.to_bigint();
    // Calculate the modulus operation, not the remainder.
    let number_of_missing_blocks = ((((-n) % batch_size) + batch_size) % batch_size)
        .to_u32()
        .expect("Failed to convert number of missing blocks to u32.");
    assert!(
        (0..20).contains(&number_of_missing_blocks),
        "number_of_missing_blocks: {number_of_missing_blocks} is expected to be in the range [0, \
         20). Got n: {n} and batch size: {batch_size}."
    );
    let sha256_input_chunk_size_felts =
        felt_to_usize(Const::Sha256InputChunkSize.fetch(constants)?)?;
    assert!(
        (0..100).contains(&sha256_input_chunk_size_felts),
        "sha256_input_chunk_size_felts: {sha256_input_chunk_size_felts} is expected to be in the \
         range [0, 100)."
    );
    let padding = calculate_padding(sha256_input_chunk_size_felts, number_of_missing_blocks);

    let sha_ptr_end = get_ptr_from_var_name(Ids::Sha256PtrEnd.into(), vm, ids_data, ap_tracking)?;
    vm.load_data(sha_ptr_end, &padding)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn segments_add_temp<S: StateReader>(
    HintArgs { vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let temp_segment = vm.add_temporary_segment();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::SegmentsAddTemp), temp_segment)
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_ap_to_actual_fee<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
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

#[allow(clippy::result_large_err)]
pub(crate) fn skip_tx<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    Ok(hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.skip_tx()?)
}

#[allow(clippy::result_large_err)]
pub(crate) fn start_tx<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let tx_type = hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?.tx_type();
    hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.start_tx(tx_type)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn os_input_transactions<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let num_txns = hint_processor.get_current_execution_helper()?.os_block_input.transactions.len();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::OsInputTransactions), num_txns)
}

#[allow(clippy::result_large_err)]
pub(crate) fn segments_add<S: StateReader>(
    HintArgs { vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let segment = vm.add_memory_segment();
    Ok(insert_value_into_ap(vm, segment)?)
}

fn calculate_padding(
    sha256_input_chunk_size_felts: usize,
    number_of_missing_blocks: u32,
) -> Vec<MaybeRelocatable> {
    let message = vec![0_u32; sha256_input_chunk_size_felts];
    let flat_message = sha2::digest::generic_array::GenericArray::from_exact_iter(
        message.iter().flat_map(|v| v.to_be_bytes()),
    )
    .expect("Failed to create a dummy message for sha2_finalize.");
    let mut initial_state = IV;
    sha2::compress256(&mut initial_state, &[flat_message]);
    let padding_to_repeat: Vec<u32> =
        [message, IV.to_vec(), initial_state.to_vec()].into_iter().flatten().collect();

    let mut padding = vec![];
    let padding_extension =
        padding_to_repeat.iter().map(|x| MaybeRelocatable::from(Felt::from(*x)));
    for _ in 0..number_of_missing_blocks {
        padding.extend(padding_extension.clone());
    }
    padding
}

#[cfg(test)]
mod tests {
    use cairo_vm::types::relocatable::MaybeRelocatable;
    use rstest::rstest;
    use starknet_types_core::felt::Felt;

    use super::calculate_padding;

    #[rstest]
    #[case(3)]
    #[case(1)]

    fn test_calculate_padding(#[case] number_of_missing_blocks: u32) {
        // The expected padding is independent of the number of missing blocks.
        let expected_single_padding: [u32; 32] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1779033703, 3144134277, 1013904242,
            2773480762, 1359893119, 2600822924, 528734635, 1541459225, 3663108286, 398046313,
            1647531929, 2006957770, 2363872401, 3235013187, 3137272298, 406301144,
        ];
        let sha256_input_chunk_size_felts = 16;
        let padding = calculate_padding(sha256_input_chunk_size_felts, number_of_missing_blocks);
        let number_of_missing_blocks = usize::try_from(number_of_missing_blocks).unwrap();
        assert!(padding.len() % number_of_missing_blocks == 0);
        let single_padding_size = padding.len() / number_of_missing_blocks;
        assert_eq!(single_padding_size, expected_single_padding.len());

        // Cast to MaybeRelocatable.
        let expected_single_padding: Vec<MaybeRelocatable> = expected_single_padding
            .iter()
            .map(|x| MaybeRelocatable::from(Felt::from(*x)))
            .collect();
        let actual_single_padding = &padding[..single_padding_size];
        assert_eq!(actual_single_padding, &expected_single_padding);
    }
}
