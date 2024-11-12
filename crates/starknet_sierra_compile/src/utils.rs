use std::clone::Clone;

use cairo_lang_starknet_classes::contract_class::{
    ContractClass as CairoLangContractClass, ContractEntryPoint as CairoLangContractEntryPoint,
    ContractEntryPoints as CairoLangContractEntryPoints,
};
use cairo_lang_utils::bigint::BigUintAsHex;
use starknet_api::rpc_transaction::{
    ContractClass as RpcContractClass, EntryPointByType as StarknetApiEntryPointByType,
};
use starknet_api::state::EntryPoint as StarknetApiEntryPoint;
use starknet_types_core::felt::Felt;

/// Retruns a [`CairoLangContractClass`] struct ready for Sierra to Casm compilation. Note the `abi`
/// field is None as it is not relevant for the compilation.
pub fn into_contract_class_for_compilation(
    rpc_contract_class: &RpcContractClass,
) -> CairoLangContractClass {
    let sierra_program =
        sierra_program_as_felts_to_big_uint_as_hex(&rpc_contract_class.sierra_program);
    let entry_points_by_type =
        into_cairo_lang_contract_entry_points(&rpc_contract_class.entry_points_by_type);

    CairoLangContractClass {
        sierra_program,
        sierra_program_debug_info: None,
        contract_class_version: rpc_contract_class.contract_class_version.clone(),
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
        selector: entry_point.selector.0.to_biguint(),
        function_idx: entry_point.function_idx.0,
    }
}

pub fn sierra_program_as_felts_to_big_uint_as_hex(sierra_program: &[Felt]) -> Vec<BigUintAsHex> {
    sierra_program.iter().map(felt_to_big_uint_as_hex).collect()
}

fn felt_to_big_uint_as_hex(felt: &Felt) -> BigUintAsHex {
    BigUintAsHex { value: felt.to_biguint() }
}
