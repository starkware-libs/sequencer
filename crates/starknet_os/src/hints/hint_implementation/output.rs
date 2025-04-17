use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn set_tree_structure<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_state_updates_start<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let use_kzg_da_felt =
        get_integer_from_var_name(Ids::UseKzgDa.into(), vm, ids_data, ap_tracking)?;

    // Set `use_kzg_da` in globals since it will be used in `process_data_availability`
    exec_scopes.insert_value(Scope::UseKzgDa.into(), use_kzg_da_felt);

    // Recompute `compress_state_updates` until this issue is fixed in our VM version:
    // https://github.com/lambdaclass/cairo-vm/issues/1897
    // TODO(Rotem): fix code when we update to VM 2.0.0 (fix should be available in one of the RCs).

    let full_output = get_integer_from_var_name(Ids::FullOutput.into(), vm, ids_data, ap_tracking)?;
    let compress_state_updates = Felt::ONE - full_output;

    let use_kzg_da = match use_kzg_da_felt {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected { id: Ids::UseKzgDa, felt: use_kzg_da_felt }),
    }?;

    let use_compress_state_updates = match compress_state_updates {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected { id: Ids::FullOutput, felt: full_output }),
    }?;

    if use_kzg_da || use_compress_state_updates {
        insert_value_from_var_name(
            Ids::StateUpdatesStart.into(),
            vm.add_memory_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    } else {
        // Assign a temporary segment, to be relocated into the output segment.
        insert_value_from_var_name(
            Ids::StateUpdatesStart.into(),
            vm.add_temporary_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    }

    Ok(())
}

pub(crate) fn set_compressed_start<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let use_kzg_da_felt = exec_scopes.get::<Felt>(Scope::UseKzgDa.into())?;

    let use_kzg_da = match use_kzg_da_felt {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected { id: Ids::UseKzgDa, felt: use_kzg_da_felt }),
    }?;

    if use_kzg_da {
        insert_value_from_var_name(
            Ids::CompressedStart.into(),
            vm.add_memory_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    } else {
        // Assign a temporary segment, to be relocated into the output segment.
        insert_value_from_var_name(
            Ids::CompressedStart.into(),
            vm.add_temporary_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    }

    Ok(())
}

pub(crate) fn set_n_updates_small<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}
