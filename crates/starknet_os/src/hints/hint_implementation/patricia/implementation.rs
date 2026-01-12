use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use num_bigint::BigUint;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    EdgeData,
    EdgePathLength,
    PathToBottom,
    Preimage,
};
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::patricia::utils::{
    build_update_tree,
    create_preimage_mapping,
    patricia_guess_descents,
    DecodeNodeCase,
    DescentMap,
    DescentStart,
    InnerNode,
    LayerIndex,
    Path,
    UpdateTree,
};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Ids, Scope};
use crate::vm_utils::{
    get_address_of_nested_fields,
    get_field_offset,
    get_size_of_cairo_struct,
    insert_value_to_nested_field,
    insert_values_to_fields,
};

pub(crate) fn set_siblings(mut ctx: HintArgs<'_>) -> OsHintResult {
    let descend: &Path = ctx.exec_scopes.get_ref(Scope::Descend.into())?;

    let length: u8 = descend.0.length.into();
    let path = descend.0.path;

    let siblings = ctx.get_ptr(Ids::Siblings)?;

    ctx.vm.insert_value(siblings, Felt::from(length))?;
    ctx.insert_value(Ids::Word, Felt::from(&path))?;

    Ok(())
}

pub(crate) fn is_case_right(ctx: HintArgs<'_>) -> OsHintResult {
    let case: DecodeNodeCase = ctx.exec_scopes.get(Scope::Case.into())?;
    let bit = ctx.get_integer(Ids::Bit)?;

    let case_right = Felt::from(case == DecodeNodeCase::Right);

    if bit != Felt::ZERO && bit != Felt::ONE {
        return Err(OsHintError::ExpectedBit { id: Ids::Bit, felt: bit });
    }

    // Felts do not support XOR, compute it manually.
    let value_felt = Felt::from(bit != case_right);
    insert_value_into_ap(ctx.vm, value_felt)?;

    Ok(())
}

pub(crate) fn set_bit<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintArgs<'_>,
) -> OsHintResult {
    let edge_path_addr = get_address_of_nested_fields(
        ctx.ids_data,
        Ids::Edge,
        CairoStruct::NodeEdge,
        ctx.vm,
        ctx.ap_tracking,
        &["path"],
        hint_processor.program,
    )?;
    let edge_path = ctx.vm.get_integer(edge_path_addr)?.into_owned();
    let new_length: u8 = Ids::NewLength.fetch_as(ctx.vm, ctx.ids_data, ctx.ap_tracking)?;

    let bit = (edge_path.to_biguint() >> new_length) & BigUint::from(1u64);
    let bit_felt = Felt::from(&bit);
    ctx.insert_value(Ids::Bit, bit_felt)?;

    Ok(())
}

pub(crate) fn set_ap_to_descend(ctx: HintArgs<'_>) -> OsHintResult {
    let descent_map: &DescentMap = ctx.exec_scopes.get_ref(Scope::DescentMap.into())?;

    let height = SubTreeHeight(Ids::Height.fetch_as(ctx.vm, ctx.ids_data, ctx.ap_tracking)?);

    // The path is from the root to the current node, so we can calculate its length.
    let path_to_upper_node = Path(PathToBottom::new(
        Ids::Path.fetch_as(ctx.vm, ctx.ids_data, ctx.ap_tracking)?,
        EdgePathLength::new(SubTreeHeight::ACTUAL_HEIGHT.0 - height.0)?,
    )?);
    let descent_start = DescentStart { height, path_to_upper_node };

    let case_descent = match descent_map.get(&descent_start) {
        None => Felt::ZERO,
        Some(path) => {
            ctx.exec_scopes.insert_value(Scope::Descend.into(), path.clone());
            Felt::ONE
        }
    };
    insert_value_into_ap(ctx.vm, case_descent)?;
    Ok(())
}

pub(crate) fn assert_case_is_right(ctx: HintArgs<'_>) -> OsHintResult {
    let case: DecodeNodeCase = ctx.exec_scopes.get(Scope::Case.into())?;
    if case != DecodeNodeCase::Right {
        return Err(OsHintError::AssertionFailed { message: "case != 'right".to_string() });
    }
    Ok(())
}

pub(crate) fn write_case_not_left_to_ap(ctx: HintArgs<'_>) -> OsHintResult {
    let case: DecodeNodeCase = ctx.exec_scopes.get(Scope::Case.into())?;
    let value = Felt::from(case != DecodeNodeCase::Left);
    insert_value_into_ap(ctx.vm, value)?;
    Ok(())
}

pub(crate) fn split_descend(mut ctx: HintArgs<'_>) -> OsHintResult {
    let descend: &Path = ctx.exec_scopes.get_ref(Scope::Descend.into())?;
    let length: u8 = descend.0.length.into();
    let path = descend.0.path;

    ctx.insert_value(Ids::Length, Felt::from(length))?;
    ctx.insert_value(Ids::Word, Felt::from(&path))?;

    Ok(())
}

pub(crate) fn height_is_zero_or_len_node_preimage_is_two<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintArgs<'_>,
) -> OsHintResult {
    let height = ctx.get_integer(Ids::Height)?;

    let answer = if height == Felt::ZERO {
        Felt::ONE
    } else {
        let node = HashOutput(ctx.get_integer(Ids::Node)?);
        let commitment_facts = &hint_processor.get_commitment_info()?.commitment_facts;
        let preimage_value = Preimage::try_from(
            commitment_facts.get(&node).ok_or(OsHintError::MissingPreimage(node))?,
        )?;
        Felt::from(preimage_value.length() == 2)
    };

    insert_value_into_ap(ctx.vm, answer)?;

    Ok(())
}

pub(crate) fn prepare_preimage_validation_non_deterministic_hashes<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintArgs<'_>,
) -> OsHintResult {
    let node: UpdateTree = ctx.exec_scopes.get(Scope::Node.into())?;
    let UpdateTree::InnerNode(inner_node) = node else {
        return Err(OsHintError::ExpectedInnerNode);
    };

    let case = inner_node.case();
    let (left_child, right_child) = inner_node.get_children();

    ctx.exec_scopes.insert_value(Scope::LeftChild.into(), left_child.clone());
    ctx.exec_scopes.insert_value(Scope::RightChild.into(), right_child.clone());
    ctx.exec_scopes.insert_value(Scope::Case.into(), case.clone());

    let ids_node = HashOutput(ctx.get_integer(Ids::Node)?);

    let commitment_facts = &hint_processor.get_commitment_info()?.commitment_facts;

    // This hint is called only when the Node is Binary.
    let preimage = Preimage::try_from(
        commitment_facts.get(&ids_node).ok_or(OsHintError::MissingPreimage(ids_node))?,
    )?;
    let binary_data = preimage.get_binary()?;

    let current_hash_address = ctx.get_ptr(Ids::CurrentHash)?;

    let nested_fields_and_values =
        [("x", binary_data.left_data.0.into()), ("y", binary_data.right_data.0.into())];
    insert_values_to_fields(
        current_hash_address,
        hint_processor.commitment_type.hash_builtin_struct(),
        ctx.vm,
        nested_fields_and_values.as_slice(),
        hint_processor.program,
    )?;

    // We don't support hash verification skipping and the scope variable
    // `__patricia_skip_validation_runner`.

    insert_value_into_ap(ctx.vm, Felt::from(case != DecodeNodeCase::Both))?;

    Ok(())
}

pub(crate) fn build_descent_map<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintArgs<'_>,
) -> OsHintResult {
    let n_updates: usize = Ids::NUpdates.fetch_as(ctx.vm, ctx.ids_data, ctx.ap_tracking)?;

    let update_ptr_address = ctx.get_ptr(Ids::UpdatePtr)?;

    let dict_access_size =
        get_size_of_cairo_struct(CairoStruct::DictAccess, hint_processor.program)?;

    let key_offset = get_field_offset(CairoStruct::DictAccess, "key", hint_processor.program)?;
    let new_value_offset =
        get_field_offset(CairoStruct::DictAccess, "new_value", hint_processor.program)?;

    let mut modifications = Vec::new();
    for i in 0..n_updates {
        let curr_update_ptr = (update_ptr_address + i * dict_access_size)?;
        let layer_index = ctx.vm.get_integer((curr_update_ptr + key_offset)?)?;
        let new_value = ctx.vm.get_integer((curr_update_ptr + new_value_offset)?)?;

        modifications.push((
            LayerIndex::new(layer_index.into_owned().to_biguint())?,
            HashOutput(new_value.into_owned()),
        ));
    }

    let height = SubTreeHeight(Ids::Height.fetch_as(ctx.vm, ctx.ids_data, ctx.ap_tracking)?);
    let node = build_update_tree(height, modifications)?;

    let commitment_facts = &hint_processor.get_commitment_info()?.commitment_facts;
    let preimage_map = create_preimage_mapping(commitment_facts)?;
    let prev_root = HashOutput(ctx.get_integer(Ids::PrevRoot)?);
    let new_root = HashOutput(ctx.get_integer(Ids::NewRoot)?);
    let descent_map = patricia_guess_descents(height, &node, &preimage_map, prev_root, new_root)?;

    ctx.exec_scopes.insert_value(Scope::Node.into(), node);
    ctx.exec_scopes.insert_value(Scope::DescentMap.into(), descent_map);

    // We do not build `common_args` as it is a Python trick to enter new scopes with a
    // dict destructuring one-liner as the dict references itself. Neat trick that does not
    // translate too well in Rust. We just make sure that `descent_map`, and `preimage` are in
    // the scope.

    // We don't support hash verification skipping and the scope variable
    // `__patricia_skip_validation_runner`.

    Ok(())
}

fn enter_scope_specific_node(node: UpdateTree, exec_scopes: &mut ExecutionScopes) -> OsHintResult {
    // No need to insert the preimage map into the scope, as we extract it directly
    // from the execution helper.
    let descent_map: DescentMap = exec_scopes.get(Scope::DescentMap.into())?;
    let new_scope = HashMap::from([
        (Scope::Node.into(), any_box!(node)),
        (Scope::DescentMap.into(), any_box!(descent_map)),
    ]);
    exec_scopes.enter_scope(new_scope);

    Ok(())
}

pub(crate) fn enter_scope_node(ctx: HintArgs<'_>) -> OsHintResult {
    let node: UpdateTree = ctx.exec_scopes.get(Scope::Node.into())?;
    enter_scope_specific_node(node, ctx.exec_scopes)
}

pub(crate) fn enter_scope_new_node(mut ctx: HintArgs<'_>) -> OsHintResult {
    let case: DecodeNodeCase = ctx.exec_scopes.get(Scope::Case.into())?;

    let (new_node, case_not_left) = match case {
        DecodeNodeCase::Left => (ctx.exec_scopes.get(Scope::LeftChild.into())?, Felt::ZERO),
        DecodeNodeCase::Right => (ctx.exec_scopes.get(Scope::RightChild.into())?, Felt::ONE),
        DecodeNodeCase::Both => {
            return Err(OsHintError::AssertionFailed {
                message: "Expected case not to be 'both'.".to_string(),
            });
        }
    };

    ctx.insert_value(Ids::ChildBit, case_not_left)?;

    enter_scope_specific_node(new_node, ctx.exec_scopes)
}

fn enter_scope_next_node_bit(is_left: bool, ctx: HintArgs<'_>) -> OsHintResult {
    let ids_bit = ctx.get_integer(Ids::Bit)?;
    let left_bit = Felt::from(is_left);

    let new_node = match ids_bit {
        x if x == left_bit => ctx.exec_scopes.get(Scope::LeftChild.into())?,
        x if x == Felt::ONE - left_bit => ctx.exec_scopes.get(Scope::RightChild.into())?,
        _ => {
            return Err(OsHintError::ExpectedBit { id: Ids::Bit, felt: ids_bit });
        }
    };
    enter_scope_specific_node(new_node, ctx.exec_scopes)
}

pub(crate) fn enter_scope_next_node_bit_0(ctx: HintArgs<'_>) -> OsHintResult {
    enter_scope_next_node_bit(false, ctx)
}

pub(crate) fn enter_scope_next_node_bit_1(ctx: HintArgs<'_>) -> OsHintResult {
    enter_scope_next_node_bit(true, ctx)
}

pub(crate) fn enter_scope_left_child(ctx: HintArgs<'_>) -> OsHintResult {
    let left_child: UpdateTree = ctx.exec_scopes.get(Scope::LeftChild.into())?;
    enter_scope_specific_node(left_child, ctx.exec_scopes)
}

pub(crate) fn enter_scope_right_child(ctx: HintArgs<'_>) -> OsHintResult {
    let right_child: UpdateTree = ctx.exec_scopes.get(Scope::RightChild.into())?;
    enter_scope_specific_node(right_child, ctx.exec_scopes)
}

pub(crate) fn enter_scope_descend_edge(ctx: HintArgs<'_>) -> OsHintResult {
    let mut new_node: UpdateTree = ctx.exec_scopes.get(Scope::Node.into())?;
    let length: u8 = Ids::Length.fetch_as(ctx.vm, ctx.ids_data, ctx.ap_tracking)?;

    // We aim to traverse downward through the node until we reach the end of the descent.
    // In this implementation, the node is of type `UpdateTree`, which is not represented as a
    // tuple. This simplifies traversal: we don't need to track the path explicitly, as we can
    // unwrap the `UpdateTree` structure `length` times.
    // It is guaranteed that none of the nodes along this path are of type `both`,
    // since such a node would break the definition of a valid descent (see `get_descents` for
    // details).
    for i in (0..length).rev() {
        let UpdateTree::InnerNode(inner_node) = new_node else {
            return Err(OsHintError::ExpectedInnerNode);
        };

        new_node = match inner_node {
            InnerNode::Left(left) => *left,
            InnerNode::Right(right) => *right,
            InnerNode::Both(_, _) => return Err(OsHintError::ExpectedSingleChild(i)),
        }
    }

    enter_scope_specific_node(new_node, ctx.exec_scopes)
}

pub(crate) fn load_edge<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintArgs<'_>,
) -> OsHintResult {
    // We don't support hash verification skipping and the scope variable
    // `__patricia_skip_validation_runner`.
    let node = HashOutput(ctx.get_integer(Ids::Node)?);
    let commitment_facts = &hint_processor.get_commitment_info()?.commitment_facts;
    let preimage =
        Preimage::try_from(commitment_facts.get(&node).ok_or(OsHintError::MissingPreimage(node))?)?;
    let Preimage::Edge(EdgeData { bottom_data: bottom_hash, path_to_bottom }) = preimage else {
        // We expect an edge node.
        return Err(OsHintError::AssertionFailed {
            message: format!("An edge node is expected, found {preimage:?}"),
        });
    };
    // Allocate space for the edge node.
    let edge_ptr = ctx.vm.add_memory_segment();
    ctx.insert_value(Ids::Edge, edge_ptr)?;
    // Fill the node fields.
    insert_values_to_fields(
        edge_ptr,
        CairoStruct::NodeEdge,
        ctx.vm,
        &[
            ("length", Felt::from(path_to_bottom.length).into()),
            ("path", Felt::from(&path_to_bottom.path).into()),
            ("bottom", bottom_hash.0.into()),
        ],
        hint_processor.program,
    )?;
    let hash_ptr = ctx.get_ptr(Ids::HashPtr)?;
    insert_value_to_nested_field(
        hash_ptr,
        hint_processor.commitment_type.hash_builtin_struct(),
        ctx.vm,
        &["result"],
        hint_processor.program,
        node.0 - Felt::from(path_to_bottom.length),
    )?;
    Ok(())
}

pub(crate) fn load_bottom<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintArgs<'_>,
) -> OsHintResult {
    let bottom_hash = HashOutput(
        ctx.vm
            .get_integer(get_address_of_nested_fields(
                ctx.ids_data,
                Ids::Edge,
                CairoStruct::NodeEdge,
                ctx.vm,
                ctx.ap_tracking,
                &["bottom"],
                hint_processor.program,
            )?)?
            .into_owned(),
    );
    let commitment_facts = &hint_processor.get_commitment_info()?.commitment_facts;
    let preimage = Preimage::try_from(
        commitment_facts.get(&bottom_hash).ok_or(OsHintError::MissingPreimage(bottom_hash))?,
    )?;
    let binary_data = preimage.get_binary()?;

    let hash_ptr_address = ctx.get_ptr(Ids::HashPtr)?;
    let nested_fields_and_values =
        [("x", binary_data.left_data.0.into()), ("y", binary_data.right_data.0.into())];
    insert_values_to_fields(
        hash_ptr_address,
        hint_processor.commitment_type.hash_builtin_struct(),
        ctx.vm,
        nested_fields_and_values.as_slice(),
        hint_processor.program,
    )?;

    // We don't support hash verification skipping and the scope variable
    // `__patricia_skip_validation_runner`.

    Ok(())
}

pub(crate) fn decode_node(ctx: HintArgs<'_>) -> OsHintResult {
    let node: UpdateTree = ctx.exec_scopes.get(Scope::Node.into())?;
    let UpdateTree::InnerNode(inner_node) = node else {
        return Err(OsHintError::ExpectedInnerNode);
    };

    let case = inner_node.case();
    let (left_child, right_child) = inner_node.get_children();

    ctx.exec_scopes.insert_value(Scope::LeftChild.into(), left_child.clone());
    ctx.exec_scopes.insert_value(Scope::RightChild.into(), right_child.clone());
    ctx.exec_scopes.insert_value(Scope::Case.into(), case.clone());

    insert_value_into_ap(ctx.vm, Felt::from(case != DecodeNodeCase::Both))?;

    Ok(())
}
