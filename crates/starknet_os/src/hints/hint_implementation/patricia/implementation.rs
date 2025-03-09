use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use num_bigint::BigUint;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::patricia::utils::{DecodeNodeCase, Preimage};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn set_siblings<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_case_right<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    let bit = get_integer_from_var_name(Ids::Bit.into(), vm, ids_data, ap_tracking)?;

    let case_felt = match case {
        DecodeNodeCase::Right => Felt::ONE,
        _ => Felt::ZERO,
    };

    // Felts do not support XOR, perform the computation on biguints.
    let value = bit.to_biguint() ^ case_felt.to_biguint();
    let value_felt = Felt::from(&value);
    insert_value_into_ap(vm, value_felt)?;

    Ok(())
}

pub(crate) fn set_bit<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let edge_path_addr = get_address_of_nested_fields(
        ids_data,
        Ids::Edge,
        CairoStruct::NodeEdge,
        vm,
        ap_tracking,
        &["path".to_string()],
        &hint_processor.execution_helper.os_program,
    )?;
    let edge_path = vm.get_integer(edge_path_addr)?.into_owned();
    let new_length = {
        let new_length =
            get_integer_from_var_name(Ids::NewLength.into(), vm, ids_data, ap_tracking)?;
        u8::try_from(new_length.to_biguint())?
    };

    let bit = (edge_path.to_biguint() >> new_length) & BigUint::from(1u64);
    let bit_felt = Felt::from(&bit);
    insert_value_from_var_name(Ids::Bit.into(), bit_felt, vm, ids_data, ap_tracking)?;

    Ok(())
}

pub(crate) fn set_ap_to_descend<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_case_is_right<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    match case {
        DecodeNodeCase::Right => Ok(()),
        _ => Err(OsHintError::AssertionFailed { message: "case != 'right".to_string() }),
    }
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
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let height = get_integer_from_var_name(Ids::Height.into(), vm, ids_data, ap_tracking)?;
    let node = get_integer_from_var_name(Ids::Node.into(), vm, ids_data, ap_tracking)?;

    let ap = if height == Felt::ZERO {
        Felt::ONE
    } else {
        let preimage: Preimage = exec_scopes.get(Scope::Preimage.into())?;
        let preimage_value = preimage
            .get(node.as_ref())
            .ok_or(OsHintError::OutOfBounds { vec_name: "preimage".to_string(), index: node })?;
        Felt::from(preimage_value.len() == 2)
    };

    insert_value_into_ap(vm, ap)?;

    Ok(())
}

pub(crate) fn prepare_preimage_validation_non_deterministic_hashes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn build_descent_map<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
