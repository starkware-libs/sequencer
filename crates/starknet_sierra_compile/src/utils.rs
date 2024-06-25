use std::clone::Clone;

use cairo_lang_starknet_classes::contract_class::{
    ContractClass as CairoLangContractClass, ContractEntryPoint as CairoLangContractEntryPoint,
    ContractEntryPoints as CairoLangContractEntryPoints,
};
use cairo_lang_utils::bigint::BigUintAsHex;
use num_bigint::BigUint;
use starknet_api::hash::StarkFelt;
use starknet_api::rpc_transaction::{
    ContractClass as StarknetApiContractClass, EntryPointByType as StarknetApiEntryPointByType,
};
use starknet_api::state::EntryPoint as StarknetApiEntryPoint;

/// Retruns a [`CairoLangContractClass`] struct ready for Sierra to Casm compilation. Note the `abi`
/// field is None as it is not relevant for the compilation.
pub fn into_contract_class_for_compilation(
    starknet_api_contract_class: &StarknetApiContractClass,
) -> CairoLangContractClass {
    let sierra_program = starknet_api_contract_class
        .sierra_program
        .iter()
        .map(stark_felt_to_big_uint_as_hex)
        .collect();
    let entry_points_by_type =
        into_cairo_lang_contract_entry_points(&starknet_api_contract_class.entry_points_by_type);

    CairoLangContractClass {
        sierra_program,
        sierra_program_debug_info: None,
        contract_class_version: starknet_api_contract_class.contract_class_version.clone(),
        entry_points_by_type,
        abi: None,
    }
}

fn into_cairo_lang_contract_entry_points(
    entry_points_by_type: &StarknetApiEntryPointByType,
) -> CairoLangContractEntryPoints {
    let StarknetApiEntryPointByType { constructor, external, l1handler } = entry_points_by_type;
    CairoLangContractEntryPoints {
        external: into_cairo_lang_contract_entry_points_vec(external),
        l1_handler: into_cairo_lang_contract_entry_points_vec(l1handler),
        constructor: into_cairo_lang_contract_entry_points_vec(constructor),
    }
}

fn into_cairo_lang_contract_entry_points_vec(
    entry_points: &[StarknetApiEntryPoint],
) -> Vec<CairoLangContractEntryPoint> {
    entry_points.iter().map(into_cairo_lang_contract_entry_point).collect()
}

fn into_cairo_lang_contract_entry_point(
    entry_point: &StarknetApiEntryPoint,
) -> CairoLangContractEntryPoint {
    CairoLangContractEntryPoint {
        selector: stark_felt_to_big_uint(&entry_point.selector.0),
        function_idx: entry_point.function_idx.0,
    }
}

fn stark_felt_to_big_uint_as_hex(stark_felt: &StarkFelt) -> BigUintAsHex {
    BigUintAsHex { value: stark_felt_to_big_uint(stark_felt) }
}

fn stark_felt_to_big_uint(stark_felt: &StarkFelt) -> BigUint {
    BigUint::from_bytes_be(stark_felt.bytes())
}
