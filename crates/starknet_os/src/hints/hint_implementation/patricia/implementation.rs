use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::patricia::utils::DecodeNodeCase;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn set_siblings<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_case_right<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    let bit = get_integer_from_var_name(Ids::Bit.into(), vm, ids_data, ap_tracking)?;

    let case_right = Felt::from(case == DecodeNodeCase::Right);

    if bit != Felt::ZERO && bit != Felt::ONE {
        return Err(OsHintError::ExpectedBit { id: Ids::Bit, felt: bit });
    }

    // Felts do not support XOR, compute it manually.
    let value_felt = bit * (Felt::ONE - case_right) + (Felt::ONE - bit) * case_right;
    insert_value_into_ap(vm, value_felt)?;

    Ok(())
}

pub(crate) fn set_bit<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_case_is_right<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    if case != DecodeNodeCase::Right {
        return Err(OsHintError::AssertionFailed { message: "case != 'right".to_string() });
    }
    Ok(())
}

pub(crate) fn write_case_not_left_to_ap<S: StateReader>(
    HintArgs { vm, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    let value = Felt::from(case != DecodeNodeCase::Left);
    insert_value_into_ap(vm, value)?;
    Ok(())
}

pub(crate) fn split_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn height_is_zero_or_len_node_preimage_is_two<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn prepare_preimage_validation_non_deterministic_hashes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn build_descent_map<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
