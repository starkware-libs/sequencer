// This module contains code taken from starknet-replay.
// For more information, see the original repository at:
// `<starknet-replay: https://github.com/lambdaclass/starknet-replay>`

use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::Arc;

use blockifier::execution::contract_class::{ContractClass, ContractClassV0, ContractClassV0Inner};
use blockifier::state::state_api::StateResult;
use cairo_lang_starknet_classes::contract_class::ContractEntryPoints;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_vm::types::program::Program;
use flate2::bufread;
use serde::Deserialize;
use starknet_api::core::EntryPointSelector;
use starknet_api::deprecated_contract_class::{EntryPointOffset, EntryPointType, EntryPointV0};
use starknet_api::hash::StarkHash;
use starknet_core::types::{
    CompressedLegacyContractClass,
    FlattenedSierraClass,
    LegacyContractEntryPoint,
    LegacyEntryPointsByType,
};
use starknet_gateway::errors::serde_err_to_state_err;

#[derive(Debug, Deserialize)]
pub struct MiddleSierraContractClass {
    pub sierra_program: Vec<BigUintAsHex>,
    pub contract_class_version: String,
    pub entry_points_by_type: ContractEntryPoints,
}

/// Maps `LegacyEntryPointsByType` to a `HashMap` where each `EntryPointType`
/// is associated with a vector of `EntryPoint`. Converts selectors and offsets
/// from legacy format to new `EntryPoint` struct.
pub fn map_entry_points_by_type_legacy(
    entry_points_by_type: LegacyEntryPointsByType,
) -> HashMap<EntryPointType, Vec<EntryPointV0>> {
    let entry_types_to_points = HashMap::from([
        (EntryPointType::Constructor, entry_points_by_type.constructor),
        (EntryPointType::External, entry_points_by_type.external),
        (EntryPointType::L1Handler, entry_points_by_type.l1_handler),
    ]);

    let to_contract_entry_point = |entrypoint: &LegacyContractEntryPoint| -> EntryPointV0 {
        let felt: StarkHash = StarkHash::from_bytes_be(&entrypoint.selector.to_bytes_be());
        EntryPointV0 {
            offset: EntryPointOffset(usize::try_from(entrypoint.offset).unwrap()),
            selector: EntryPointSelector(felt),
        }
    };

    let mut entry_points_by_type_map = HashMap::new();
    for (entry_point_type, entry_points) in entry_types_to_points.into_iter() {
        let values = entry_points.iter().map(to_contract_entry_point).collect::<Vec<_>>();
        entry_points_by_type_map.insert(entry_point_type, values);
    }

    entry_points_by_type_map
}

/// Uncompresses a Gz Encoded vector of bytes and returns a string or error
/// Here &[u8] implements BufRead
pub fn decode_reader(bytes: Vec<u8>) -> io::Result<String> {
    let mut gz = bufread::GzDecoder::new(&bytes[..]);
    let mut s = String::new();
    gz.read_to_string(&mut s)?;
    Ok(s)
}

/// Compile a FlattenedSierraClass to a ContractClass V1 (casm) using cairo_lang_starknet_classes.
pub fn sierra_to_contact_class_v1(sierra: FlattenedSierraClass) -> StateResult<ContractClass> {
    let middle_sierra: MiddleSierraContractClass = {
        let v = serde_json::to_value(sierra).map_err(serde_err_to_state_err);
        serde_json::from_value(v?).map_err(serde_err_to_state_err)?
    };
    let sierra = cairo_lang_starknet_classes::contract_class::ContractClass {
        sierra_program: middle_sierra.sierra_program,
        contract_class_version: middle_sierra.contract_class_version,
        entry_points_by_type: middle_sierra.entry_points_by_type,
        sierra_program_debug_info: None,
        abi: None,
    };

    let casm =
        cairo_lang_starknet_classes::casm_contract_class::CasmContractClass::from_contract_class(
            sierra,
            false,
            usize::MAX,
        )
        .unwrap();
    Ok(ContractClass::V1(casm.try_into().unwrap()))
}

/// Compile a CompressedLegacyContractClass to a ContractClass V0 using cairo_lang_starknet_classes.
pub fn legacy_to_contract_class_v0(
    legacy: CompressedLegacyContractClass,
) -> StateResult<ContractClass> {
    let as_str = decode_reader(legacy.program).unwrap();
    let program = Program::from_bytes(as_str.as_bytes(), None).unwrap();
    let entry_points_by_type = map_entry_points_by_type_legacy(legacy.entry_points_by_type);
    let inner = Arc::new(ContractClassV0Inner { program, entry_points_by_type });
    Ok(ContractClass::V0(ContractClassV0(inner)))
}
