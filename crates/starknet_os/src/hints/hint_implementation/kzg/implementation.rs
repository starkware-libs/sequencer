use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use num_bigint::BigInt;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::kzg::utils::polynomial_coefficients_to_kzg_commitment;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

pub(crate) fn store_da_segment<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    log::debug!("Executing store_da_segment hint.");
    let state_updates_start =
        get_ptr_from_var_name(Ids::StateUpdatesStart.into(), vm, ids_data, ap_tracking)?;
    let da_size_felt = get_integer_from_var_name(Ids::DaSize.into(), vm, ids_data, ap_tracking)?;
    let da_size =
        usize::try_from(da_size_felt.to_biguint()).map_err(|error| OsHintError::IdsConversion {
            variant: Ids::DaSize,
            felt: da_size_felt,
            ty: "usize".to_string(),
            reason: error.to_string(),
        })?;

    let da_segment: Vec<Felt> =
        vm.get_integer_range(state_updates_start, da_size)?.into_iter().map(|s| *s).collect();

    let blob_length_felt = Const::BlobLength.fetch(constants)?;
    let blob_length = usize::try_from(blob_length_felt.to_biguint()).map_err(|error| {
        OsHintError::ConstConversion {
            variant: Const::BlobLength,
            felt: *blob_length_felt,
            ty: "usize".to_string(),
            reason: error.to_string(),
        }
    })?;

    let kzg_commitments_bigints: Vec<(BigInt, BigInt)> = da_segment
        .chunks(blob_length)
        .enumerate()
        .map(|(chunk_id, chunk)| {
            let coefficients: Vec<BigInt> = chunk.iter().map(|f| f.to_bigint()).collect();
            log::debug!("Computing KZG commitment on chunk {chunk_id}...");
            polynomial_coefficients_to_kzg_commitment(coefficients)
        })
        .collect::<Result<_, _>>()?;
    log::debug!("Done computing KZG commitments.");
    let kzg_commitments: Vec<(Felt, Felt)> = kzg_commitments_bigints
        .into_iter()
        .map(|bigint_pair| (bigint_pair.0.into(), bigint_pair.1.into()))
        .collect();

    hint_processor.set_da_segment(da_segment)?;

    let n_blobs = kzg_commitments.len();
    let kzg_commitments_segment = vm.add_temporary_segment();
    let evals_segment = vm.add_temporary_segment();

    insert_value_from_var_name(Ids::NBlobs.into(), n_blobs, vm, ids_data, ap_tracking)?;
    insert_value_from_var_name(
        Ids::KzgCommitments.into(),
        kzg_commitments_segment,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(Ids::Evals.into(), evals_segment, vm, ids_data, ap_tracking)?;

    let kzg_commitments_flattened: Vec<MaybeRelocatable> =
        kzg_commitments.into_iter().flat_map(|c| [c.0.into(), c.1.into()]).collect();
    vm.write_arg(kzg_commitments_segment, &kzg_commitments_flattened)?;

    Ok(())
}
