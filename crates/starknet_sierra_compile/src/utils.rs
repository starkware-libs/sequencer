use std::clone::Clone;
use std::io::Write;

use crate::errors::CompilationUtilError;
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
use tempfile::NamedTempFile;

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

pub(crate) fn save_contract_class_to_temp_file(
    contract_class: CairoLangContractClass,
) -> Result<NamedTempFile, CompilationUtilError> {
    let serialized_contract_class = serde_json::to_string(&contract_class)?;

    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(serialized_contract_class.as_bytes())?;
    Ok(temp_file)
}

pub(crate) fn process_compile_command_output(
    compile_output: std::process::Output,
) -> Result<Vec<u8>, CompilationUtilError> {
    if !compile_output.status.success() {
        let stderr_output = String::from_utf8(compile_output.stderr)
            .unwrap_or("Failed to get stderr output".into());
        return Err(CompilationUtilError::CompilationError(stderr_output));
    };
    Ok(compile_output.stdout)
}
