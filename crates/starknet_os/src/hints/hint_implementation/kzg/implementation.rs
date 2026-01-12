use ark_bls12_381::Fr;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::kzg::utils::{
    polynomial_coefficients_to_kzg_commitment,
    split_bigint3,
};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

pub(crate) fn guess_kzg_commitments_and_store_da_segment<
    'program,
    CHP: CommonHintProcessor<'program>,
>(
    hint_processor: &mut CHP,
    mut ctx: HintArgs<'_>,
) -> OsHintResult {
    log::debug!("Executing guess_kzg_commitments_and_store_da_segment hint.");
    let state_updates_start = ctx.get_ptr(Ids::StateUpdatesStart)?;
    let da_size_felt = ctx.get_integer(Ids::DaSize)?;
    let da_size =
        usize::try_from(da_size_felt.to_biguint()).map_err(|error| OsHintError::IdsConversion {
            variant: Ids::DaSize,
            felt: da_size_felt,
            ty: "usize".to_string(),
            reason: error.to_string(),
        })?;

    let da_segment: Vec<Felt> =
        ctx.vm.get_integer_range(state_updates_start, da_size)?.into_iter().map(|s| *s).collect();

    let blob_length_felt = Const::BlobLength.fetch(ctx.constants)?;
    let blob_length = usize::try_from(blob_length_felt.to_biguint()).map_err(|error| {
        OsHintError::ConstConversion {
            variant: Const::BlobLength,
            felt: *blob_length_felt,
            ty: "usize".to_string(),
            reason: error.to_string(),
        }
    })?;

    let kzg_commitments: Vec<(Felt, Felt)> = da_segment
        .chunks(blob_length)
        .enumerate()
        .map(|(chunk_id, chunk)| {
            let coefficients: Vec<Fr> = chunk.iter().map(|f| Fr::from(f.to_biguint())).collect();
            log::debug!("Computing KZG commitment on chunk {chunk_id}...");
            polynomial_coefficients_to_kzg_commitment(coefficients)
        })
        .collect::<Result<_, _>>()?;
    log::debug!("Done computing KZG commitments.");

    hint_processor.set_da_segment(da_segment)?;

    let n_blobs = kzg_commitments.len();
    let kzg_commitments_segment = ctx.vm.add_temporary_segment();
    let evals_segment = ctx.vm.add_temporary_segment();

    ctx.insert_value(Ids::NBlobs, n_blobs)?;
    ctx.insert_value(Ids::KzgCommitments, kzg_commitments_segment)?;
    ctx.insert_value(Ids::Evals, evals_segment)?;

    let kzg_commitments_flattened: Vec<MaybeRelocatable> =
        kzg_commitments.into_iter().flat_map(|c| [c.0.into(), c.1.into()]).collect();
    ctx.vm.write_arg(kzg_commitments_segment, &kzg_commitments_flattened)?;

    Ok(())
}

pub(crate) fn write_split_result(ctx: HintArgs<'_>) -> OsHintResult {
    let value = ctx.get_integer(Ids::Value)?.to_bigint();
    let res_ptr = ctx.get_relocatable(Ids::Res)?;

    let splits: Vec<MaybeRelocatable> =
        split_bigint3(value)?.into_iter().map(MaybeRelocatable::Int).collect();
    ctx.vm.write_arg(res_ptr, &splits)?;

    Ok(())
}
