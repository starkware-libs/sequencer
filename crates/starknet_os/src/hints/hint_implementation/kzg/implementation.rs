use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;

pub(crate) fn store_da_segment<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_updates_start =
        get_ptr_from_var_name(vars::ids::STATE_UPDATES_START, vm, ids_data, ap_tracking)?;
    let da_size =
        get_integer_from_var_name(vars::ids::DA_SIZE, vm, ids_data, ap_tracking)?.to_biguint();
    let da_size: usize = da_size.try_into().map_err(|_| HintError::BigintToU32Fail)?;

    let da_segment: Vec<Felt> =
        vm.get_integer_range(state_updates_start, da_size)?.into_iter().map(|s| *s).collect();

    let blob_length = get_constant(vars::ids::BLOB_LENGTH, constants)?.to_biguint();
    let blob_length: usize = blob_length.try_into().map_err(|_| HintError::BigintToU32Fail)?;

    let kzg_commitments: Vec<(Felt, Felt)> = da_segment
        .chunks(blob_length)
        .map(|chunk| {
            let coefficients: Vec<BigInt> = chunk.iter().map(|f| f.to_bigint()).collect();
            let res: (BigInt, BigInt) =
                polynomial_coefficients_to_kzg_commitment(coefficients).unwrap(); // TODO: unwrap
            (res.0.into(), res.1.into())
        })
        .collect();

    let ehw = exec_scopes.get::<ExecutionHelperWrapper<PCS>>(vars::scopes::EXECUTION_HELPER)?;
    let kzg_manager = &mut ehw.execution_helper.write().await.kzg_manager;
    kzg_manager.store_da_segment(da_segment)?;

    let n_blobs = kzg_commitments.len();
    let kzg_commitments_segment = vm.add_temporary_segment();
    let evals_segment = vm.add_temporary_segment();

    insert_value_from_var_name(vars::ids::N_BLOBS, n_blobs, vm, ids_data, ap_tracking)?;
    insert_value_from_var_name(
        vars::ids::KZG_COMMITMENTS,
        kzg_commitments_segment,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(vars::ids::EVALS, evals_segment, vm, ids_data, ap_tracking)?;

    let kzg_commitments_flattened: Vec<MaybeRelocatable> =
        kzg_commitments.into_iter().flat_map(|c| [c.0.into(), c.1.into()]).collect();
    vm.write_arg(kzg_commitments_segment, &kzg_commitments_flattened)?;

    Ok(())
}
