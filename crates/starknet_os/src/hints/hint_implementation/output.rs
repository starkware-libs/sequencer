use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::Felt252;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;

pub(crate) fn set_tree_structure<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_state_updates_start<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> HintResult {
    let use_kzg_da_felt =
        get_integer_from_var_name(vars::ids::USE_KZG_DA, vm, ids_data, ap_tracking)?;

    // Set `use_kzg_da` in globals since it will be used in `process_data_availability`
    exec_scopes.insert_value(vars::scopes::USE_KZG_DA, use_kzg_da_felt);

    // Recompute `compress_state_updates` until this issue is fixed
    // https://github.com/lambdaclass/cairo-vm/issues/1897
    let full_output = get_integer_from_var_name(vars::ids::FULL_OUTPUT, vm, ids_data, ap_tracking)?;
    let compress_state_updates = Felt252::ONE - full_output;

    let use_kzg_da = match use_kzg_da_felt {
        x if x == Felt252::ONE => Ok(true),
        x if x == Felt252::ZERO => Ok(false),
        _ => Err(HintError::CustomHint(
            "ids.use_kzg_da is not a boolean".to_string().into_boxed_str(),
        )),
    }?;

    let use_compress_state_updates = match compress_state_updates {
        x if x == Felt252::ONE => Ok(true),
        x if x == Felt252::ZERO => Ok(false),
        _ => Err(HintError::CustomHint(
            "ids.compress_state_updates is not a boolean".to_string().into_boxed_str(),
        )),
    }?;

    if use_kzg_da || use_compress_state_updates {
        insert_value_from_var_name(
            vars::ids::STATE_UPDATES_START,
            vm.add_memory_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    } else {
        // Assign a temporary segment, to be relocated into the output segment.
        insert_value_from_var_name(
            vars::ids::STATE_UPDATES_START,
            vm.add_temporary_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    }

    Ok(())
}

pub(crate) fn set_compressed_start<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_n_updates_small<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
