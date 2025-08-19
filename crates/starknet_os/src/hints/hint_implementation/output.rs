use std::cmp::min;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids, Scope};

const MAX_PAGE_SIZE: usize = 3800;
const OUTPUT_ATTRIBUTE_FACT_TOPOLOGY: &str = "gps_fact_topology";

pub(crate) fn set_tree_structure<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    if !hint_processor.get_serialize_data_availability_create_pages() {
        return Ok(());
    }
    let onchain_data_start = get_ptr_from_var_name(Ids::DaStart.into(), vm, ids_data, ap_tracking)?;
    let output_ptr = get_ptr_from_var_name(Ids::OutputPtr.into(), vm, ids_data, ap_tracking)?;
    let onchain_data_size = (output_ptr - onchain_data_start)?;
    let output_builtin = vm.get_output_builtin_mut()?;

    let n_pages = onchain_data_size.div_ceil(MAX_PAGE_SIZE);
    for i in 0..n_pages {
        let start_offset = i * MAX_PAGE_SIZE;
        let page_id = i + 1;
        let page_start = (onchain_data_start + start_offset)?;
        let page_size = min(onchain_data_size - start_offset, MAX_PAGE_SIZE);
        output_builtin.add_page(page_id, page_start, page_size)?;
    }
    output_builtin.add_attribute(
        OUTPUT_ATTRIBUTE_FACT_TOPOLOGY.to_string(),
        vec![
            // Push 1 + n_pages pages (all of the pages).
            1 + n_pages,
            // Create a parent node for the last n_pages.
            n_pages,
            // Don't push additional pages.
            0,
            // Take the first page (the main part) and the node that was created (onchain data)
            // and use them to construct the root of the fact tree.
            2,
        ],
    );

    Ok(())
}

pub(crate) fn set_state_updates_start(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let use_kzg_da_felt =
        get_integer_from_var_name(Ids::UseKzgDa.into(), vm, ids_data, ap_tracking)?;

    // Set `use_kzg_da` in globals since it will be used in `process_data_availability`
    exec_scopes.insert_value(Scope::UseKzgDa.into(), use_kzg_da_felt);

    let compress_state_updates =
        get_integer_from_var_name(Ids::CompressStateUpdates.into(), vm, ids_data, ap_tracking)?;

    let use_kzg_da = match use_kzg_da_felt {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected { id: Ids::UseKzgDa, felt: use_kzg_da_felt }),
    }?;

    let use_compress_state_updates = match compress_state_updates {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected {
            id: Ids::CompressStateUpdates,
            felt: compress_state_updates,
        }),
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

pub(crate) fn set_compressed_start(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_>,
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

pub(crate) fn set_n_updates_small(
    HintArgs { vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_>,
) -> OsHintResult {
    let n_updates = get_integer_from_var_name(Ids::NUpdates.into(), vm, ids_data, ap_tracking)?;
    let n_updates_small_packing_bounds =
        Const::fetch(&Const::NUpdatesSmallPackingBound, constants)?;
    insert_value_from_var_name(
        Ids::IsNUpdatesSmall.into(),
        Felt::from(&n_updates < n_updates_small_packing_bounds),
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn sha256_hash_compressed_data_with_random(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let compressed_start =
        get_ptr_from_var_name(Ids::CompressedStart.into(), vm, ids_data, ap_tracking)?;
    let compressed_dst =
        get_ptr_from_var_name(Ids::CompressedDst.into(), vm, ids_data, ap_tracking)?;
    let array_size = (compressed_dst - compressed_start)?;

    // Generate a cryptographically secure random seed
    let mut random_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut random_bytes);

    let mut hasher = Sha256::new();
    hasher.update(random_bytes);

    for i in 0..array_size {
        let felt = vm.get_integer((compressed_start + i)?)?;
        hasher.update(felt.to_bytes_be());
    }
    let hash_result = hasher.finalize();

    let mut symmetric_key_input = hash_result.to_vec();
    symmetric_key_input.push(0);
    let symmetric_key_hash = Sha256::digest(&symmetric_key_input);
    let mut symmetric_key_bytes = [0u8; 32];
    symmetric_key_bytes.copy_from_slice(&symmetric_key_hash[..]);
    let symmetric_key = Felt::from_bytes_be(&symmetric_key_bytes);
    insert_value_from_var_name(Ids::SymmetricKey.into(), symmetric_key, vm, ids_data, ap_tracking)?;

    let mut sn_private_key_1_input = hash_result.to_vec();
    sn_private_key_1_input.push(1);
    let sn_private_key_1_hash = Sha256::digest(&sn_private_key_1_input);
    // Use only first 31 bytes (248 bits) to ensure result is < 2^248 < EC group order.
    let mut sn_private_key_1_bytes = [0u8; 32];
    sn_private_key_1_bytes[1..].copy_from_slice(&sn_private_key_1_hash[..31]);
    let sn_private_key_1 = Felt::from_bytes_be(&sn_private_key_1_bytes);
    insert_value_from_var_name(
        Ids::SnPrivateKey1.into(),
        sn_private_key_1,
        vm,
        ids_data,
        ap_tracking,
    )?;

    Ok(())
}
