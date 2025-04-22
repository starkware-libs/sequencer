use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    get_relocatable_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use num_bigint::BigUint;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    EdgeData,
    EdgePathLength,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::patricia::utils::{
    build_update_tree,
    patricia_guess_descents,
    DecodeNodeCase,
    DescentMap,
    DescentStart,
    LayerIndex,
    Path,
    Preimage,
    PreimageMap,
    UpdateTree,
};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{
    get_address_of_nested_fields,
    get_field_offset,
    insert_value_to_nested_field,
    insert_values_to_fields,
};

pub(crate) fn set_siblings<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let descend: &Path = exec_scopes.get_ref(Scope::Descend.into())?;

    let length: u8 = descend.0.length.into();
    let path = descend.0.path;

    let siblings = get_ptr_from_var_name(Ids::Siblings.into(), vm, ids_data, ap_tracking)?;

    vm.insert_value(siblings, Felt::from(length))?;
    insert_value_from_var_name(Ids::Word.into(), Felt::from(&path), vm, ids_data, ap_tracking)?;

    Ok(())
}

pub(crate) fn is_case_right<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
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
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let edge_path_addr = get_address_of_nested_fields(
        ids_data,
        Ids::Edge,
        CairoStruct::NodeEdge,
        vm,
        ap_tracking,
        &["path"],
        hint_processor.os_program,
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

pub(crate) fn set_ap_to_descend<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let descent_map: &DescentMap = exec_scopes.get_ref(Scope::DescentMap.into())?;

    let height = {
        let ids_height = get_integer_from_var_name(Ids::Height.into(), vm, ids_data, ap_tracking)?;
        SubTreeHeight(u8::try_from(ids_height).map_err(|error| OsHintError::IdsConversion {
            variant: Ids::Height,
            felt: ids_height,
            ty: "u8".to_string(),
            reason: error.to_string(),
        })?)
    };
    let path_to_upper_node = {
        let ids_path = get_integer_from_var_name(Ids::Path.into(), vm, ids_data, ap_tracking)?;
        // The path is from the root to the current node, so we can calculate its length.
        Path(PathToBottom::new(
            ids_path.into(),
            EdgePathLength::new(SubTreeHeight::ACTUAL_HEIGHT.0 - height.0)?,
        )?)
    };
    let descent_start = DescentStart { height, path_to_upper_node };

    let case_descent = match descent_map.get(&descent_start) {
        None => Felt::ZERO,
        Some(path) => {
            exec_scopes.insert_value(Scope::Descend.into(), path.clone());
            Felt::ONE
        }
    };
    insert_value_into_ap(vm, case_descent)?;
    Ok(())
}

pub(crate) fn assert_case_is_right<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    if case != DecodeNodeCase::Right {
        return Err(OsHintError::AssertionFailed { message: "case != 'right".to_string() });
    }
    Ok(())
}

pub(crate) fn write_case_not_left_to_ap<S: StateReader>(
    HintArgs { vm, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let case: DecodeNodeCase = exec_scopes.get(Scope::Case.into())?;
    let value = Felt::from(case != DecodeNodeCase::Left);
    insert_value_into_ap(vm, value)?;
    Ok(())
}

pub(crate) fn split_descend<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let descend: &Path = exec_scopes.get_ref(Scope::Descend.into())?;
    let length: u8 = descend.0.length.into();
    let path = descend.0.path;

    insert_value_from_var_name(Ids::Length.into(), Felt::from(length), vm, ids_data, ap_tracking)?;
    insert_value_from_var_name(Ids::Word.into(), Felt::from(&path), vm, ids_data, ap_tracking)?;

    Ok(())
}

pub(crate) fn height_is_zero_or_len_node_preimage_is_two<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
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
    HintArgs { hint_processor, vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let node: UpdateTree = exec_scopes.get(Scope::Node.into())?;
    let UpdateTree::InnerNode(inner_node) = node else {
        return Err(OsHintError::ExpectedInnerNode);
    };

    let case = inner_node.case();
    let (left_child, right_child) = inner_node.get_children();

    exec_scopes.insert_value(Scope::LeftChild.into(), left_child.clone());
    exec_scopes.insert_value(Scope::RightChild.into(), right_child.clone());
    exec_scopes.insert_value(Scope::Case.into(), case.clone());

    let ids_node =
        HashOutput(get_integer_from_var_name(Ids::Node.into(), vm, ids_data, ap_tracking)?);

    let preimage_map: &PreimageMap = exec_scopes.get_ref(Scope::Preimage.into())?;

    // This hint is called only when the Node is Binary.
    let binary_data =
        preimage_map.get(&ids_node).ok_or(OsHintError::MissingPreimage(ids_node))?.get_binary()?;

    let hash_ptr_address =
        get_relocatable_from_var_name(Ids::CurrentHash.into(), vm, ids_data, ap_tracking)?;

    let nested_fields_and_values =
        [("x", binary_data.left_hash.0.into()), ("y", binary_data.right_hash.0.into())];
    insert_values_to_fields(
        hash_ptr_address,
        hint_processor.commitment_type.hash_builtin_struct(),
        vm,
        nested_fields_and_values.as_slice(),
        hint_processor.os_program,
    )?;

    // TODO(Rotem): Verify that it's OK to ignore the scope variable
    // `__patricia_skip_validation_runner`.

    insert_value_into_ap(vm, Felt::from(case != DecodeNodeCase::Both))?;

    Ok(())
}

pub(crate) fn build_descent_map<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let n_updates = {
        let ids_n_updates =
            get_integer_from_var_name(Ids::NUpdates.into(), vm, ids_data, ap_tracking)?;
        usize::try_from(ids_n_updates).map_err(|error| OsHintError::IdsConversion {
            variant: Ids::NUpdates,
            felt: ids_n_updates,
            ty: "usize".to_string(),
            reason: error.to_string(),
        })?
    };

    let update_ptr_address =
        get_relocatable_from_var_name(Ids::UpdatePtr.into(), vm, ids_data, ap_tracking)?;

    // TODO(Rotem): Implement a `get_size_of_cairo_struct` function when upgrading to Cairo VM 2.0.0
    // which adds a `size` field to `Identifier`.
    // https://github.com/lambdaclass/cairo-vm/blob/main/vm/src/serde/deserialize_program.rs#L105
    let dict_access_size = 3;

    let key_offset = get_field_offset(CairoStruct::DictAccess, "key", hint_processor.os_program)?;
    let new_value_offset =
        get_field_offset(CairoStruct::DictAccess, "new_value", hint_processor.os_program)?;

    let mut modifications = Vec::new();
    for i in 0..n_updates {
        let curr_update_ptr = (update_ptr_address + i * dict_access_size)?;
        let layer_index = vm.get_integer((curr_update_ptr + key_offset)?)?;
        let new_value = vm.get_integer((curr_update_ptr + new_value_offset)?)?;

        modifications.push((
            LayerIndex::new(layer_index.into_owned().to_biguint())?,
            HashOutput(new_value.into_owned()),
        ));
    }

    let height = {
        let ids_height = get_integer_from_var_name(Ids::Height.into(), vm, ids_data, ap_tracking)?;
        SubTreeHeight(u8::try_from(ids_height).map_err(|error| OsHintError::IdsConversion {
            variant: Ids::Height,
            felt: ids_height,
            ty: "u8".to_string(),
            reason: error.to_string(),
        })?)
    };
    let node = build_update_tree(height, modifications)?;

    let preimage_map: &PreimageMap = exec_scopes.get_ref(Scope::Preimage.into())?;
    let prev_root =
        HashOutput(get_integer_from_var_name(Ids::PrevRoot.into(), vm, ids_data, ap_tracking)?);
    let new_root =
        HashOutput(get_integer_from_var_name(Ids::NewRoot.into(), vm, ids_data, ap_tracking)?);
    let descent_map = patricia_guess_descents(height, &node, preimage_map, prev_root, new_root)?;

    exec_scopes.insert_value(Scope::Node.into(), node);
    exec_scopes.insert_value(Scope::DescentMap.into(), descent_map);

    // We do not build `common_args` as it is a Python trick to enter new scopes with a
    // dict destructuring one-liner as the dict references itself. Neat trick that does not
    // translate too well in Rust. We just make sure that `descent_map`, and `preimage` are in
    // the scope.

    // TODO(Rotem): If and when hash verification skipping is supported, take
    // `__patricia_skip_validation_runner` into account.

    Ok(())
}

pub(crate) fn enter_scope_node<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_new_node<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_0<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_1<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_left_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_right_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_descend_edge<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn load_edge<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // TODO(Nimrod): Verify that it's ok to ignore the scope variable
    // `__patricia_skip_validation_runner`.
    let node = HashOutput(get_integer_from_var_name(Ids::Node.into(), vm, ids_data, ap_tracking)?);
    let preimage_mapping: &PreimageMap = exec_scopes.get_ref(Scope::Preimage.into())?;
    let preimage = preimage_mapping.get(&node).ok_or(OsHintError::MissingPreimage(node))?;
    let Preimage::Edge(EdgeData { bottom_hash, path_to_bottom }) = preimage else {
        // We expect an edge node.
        return Err(OsHintError::AssertionFailed {
            message: format!("An edge node is expected, found {preimage:?}"),
        });
    };
    // Allocate space for the edge node.
    let edge_ptr = vm.add_memory_segment();
    insert_value_from_var_name(Ids::Edge.into(), edge_ptr, vm, ids_data, ap_tracking)?;
    // Fill the node fields.
    insert_values_to_fields(
        edge_ptr,
        CairoStruct::NodeEdge,
        vm,
        &[
            ("length", Felt::from(path_to_bottom.length).into()),
            ("path", Felt::from(&path_to_bottom.path).into()),
            ("bottom", bottom_hash.0.into()),
        ],
        hint_processor.os_program,
    )?;
    let hash_ptr = get_relocatable_from_var_name(Ids::HashPtr.into(), vm, ids_data, ap_tracking)?;
    insert_value_to_nested_field(
        hash_ptr,
        hint_processor.commitment_type.hash_builtin_struct(),
        vm,
        &["result"],
        hint_processor.os_program,
        node.0 - Felt::from(path_to_bottom.length),
    )?;
    Ok(())
}

pub(crate) fn load_bottom<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn decode_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}
