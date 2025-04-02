use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::patricia::utils::{DecodeNodeCase, PreimageMap};
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

    let case_right = Felt::from(case == DecodeNodeCase::Right);

    if bit != Felt::ZERO && bit != Felt::ONE {
        return Err(OsHintError::ExpectedBit { id: Ids::Bit, felt: bit });
    }

    // Felts do not support XOR, compute it manually.
    let value_felt = Felt::from(bit != case_right);
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
        &["path"],
        &hint_processor.os_program,
    )?;
    let edge_path = vm.get_integer(edge_path_addr)?.into_owned();
    let new_length = u8::try_from(
        get_integer_from_var_name(Ids::NewLength.into(), vm, ids_data, ap_tracking)?.to_biguint(),
    )?;

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
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let height = get_integer_from_var_name(Ids::Height.into(), vm, ids_data, ap_tracking)?;

    let answer = if height == Felt::ZERO {
        Felt::ONE
    } else {
        let node =
            HashOutput(get_integer_from_var_name(Ids::Node.into(), vm, ids_data, ap_tracking)?);
        let preimage_map: &PreimageMap = exec_scopes.get_ref(Scope::Preimage.into())?;
        let preimage_value = preimage_map.get(&node).ok_or(OsHintError::MissingPreimage(node))?;
        Felt::from(preimage_value.length() == 2)
    };

    insert_value_into_ap(vm, answer)?;

    Ok(())
}

pub(crate) fn prepare_preimage_validation_non_deterministic_hashes<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let (x_offset, y_offset, result_offset) = get_hash_builtin_fields(exec_scopes)?;

    let node: UpdateTree<StorageLeaf> = exec_scopes.get(vars::scopes::NODE)?;
    let node = node.ok_or(HintError::AssertionFailed("'node' should not be None".to_string().into_boxed_str()))?;

    let preimage: Preimage = exec_scopes.get(vars::scopes::PREIMAGE)?;

    let ids_node = get_integer_from_var_name(vars::ids::NODE, vm, ids_data, ap_tracking)?;

    let DecodedNode { left_child, right_child, case } = decode_node(&node)?;

    exec_scopes.insert_value(vars::scopes::LEFT_CHILD, left_child.clone());
    exec_scopes.insert_value(vars::scopes::RIGHT_CHILD, right_child.clone());
    exec_scopes.insert_value(vars::scopes::CASE, case.clone());

    let node_preimage =
        preimage.get(&ids_node).ok_or(HintError::CustomHint("Node preimage not found".to_string().into_boxed_str()))?;
    let left_hash = node_preimage[0];
    let right_hash = node_preimage[1];

    // Fill non deterministic hashes.
    let hash_ptr = get_ptr_from_var_name(vars::ids::CURRENT_HASH, vm, ids_data, ap_tracking)?;
    // memory[hash_ptr + ids.HashBuiltin.x] = left_hash
    vm.insert_value((hash_ptr + x_offset)?, left_hash)?;
    // memory[hash_ptr + ids.HashBuiltin.y] = right_hash
    vm.insert_value((hash_ptr + y_offset)?, right_hash)?;

    let hash_result_address = (hash_ptr + result_offset)?;
    skip_verification_if_configured(exec_scopes, hash_result_address)?;

    // memory[ap] = int(case != 'both')"#
    let ap = match case {
        DecodeNodeCase::Both => Felt252::ZERO,
        _ => Felt252::ONE,
    };
    insert_value_into_ap(vm, ap)?;

    Ok(())
}

pub(crate) fn build_descent_map<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
