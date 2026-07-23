// This module contains code taken from starknet-replay.
// For more information, see the original repository at:
// `<starknet-replay: https://github.com/lambdaclass/starknet-replay>`

use std::collections::HashMap;
use std::io::{self, Read};

use apollo_compilation_utils::class_utils::into_contract_class_for_compilation;
use apollo_sierra_compilation_config::config::DEFAULT_MAX_BYTECODE_SIZE;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateResult;
use cairo_lang_starknet_classes::allowed_libfuncs::ListSelector;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use flate2::bufread;
use starknet_api::contract_class::{ContractClass, EntryPointType, SierraVersion};
use starknet_api::core::EntryPointSelector;
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointOffset,
    EntryPointV0,
    Program,
};
use starknet_api::hash::StarkHash;
use starknet_api::state::SierraContractClass;
use starknet_core::types::{
    CompressedLegacyContractClass,
    LegacyContractEntryPoint,
    LegacyEntryPointsByType,
};

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod test;

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

/// Compile a SierraContractClass to a versioned ContractClass V1 (casm) in-process, using the
/// Sierra→Casm compiler as a library. The classes compiled here are fetched from the chain, so
/// unlike the sequencer gateway (which compiles user-submitted classes in a resource-limited
/// subprocess), no process isolation is needed.
pub fn sierra_to_versioned_contract_class_v1(
    sierra_contract: SierraContractClass,
) -> StateResult<(ContractClass, SierraVersion)> {
    let sierra_version = SierraVersion::extract_from_program(&sierra_contract.sierra_program)
        .map_err(|err| {
            StateError::StateReadError(format!("Failed to extract Sierra version: {err}"))
        })?;
    let contract_class_for_compilation = into_contract_class_for_compilation(&sierra_contract);
    let extracted_sierra_program =
        contract_class_for_compilation.extract_sierra_program(false).map_err(|err| {
            StateError::StateReadError(format!("Failed to extract the Sierra program: {err}"))
        })?;
    // Re-execution must accept any class the network accepted; do not restrict to the audited
    // libfuncs list.
    extracted_sierra_program
        .validate_version_compatible(ListSelector::ListName("all".to_string()))
        .map_err(|err| {
            StateError::StateReadError(format!("Sierra program version validation failed: {err}"))
        })?;
    let casm_contract_class = CasmContractClass::from_contract_class(
        contract_class_for_compilation,
        extracted_sierra_program,
        // Add pythonic hints, as the sequencer does when compiling declared classes.
        true,
        // Generous bound; declared classes already passed the network's bytecode-size limit.
        10 * DEFAULT_MAX_BYTECODE_SIZE,
    )
    .map_err(|err| {
        StateError::StateReadError(format!("Failed to compile Sierra to Casm: {err}"))
    })?;

    Ok((ContractClass::V1((casm_contract_class, sierra_version.clone())), sierra_version))
}

/// Compile a CompressedLegacyContractClass to a ContractClass V0 using cairo_lang_starknet_classes.
pub fn legacy_to_contract_class_v0(
    legacy: CompressedLegacyContractClass,
) -> StateResult<ContractClass> {
    let as_str = decode_reader(legacy.program).unwrap();
    let program: Program = serde_json::from_str(&as_str).unwrap();
    let entry_points_by_type = map_entry_points_by_type_legacy(legacy.entry_points_by_type);
    Ok((DeprecatedContractClass { program, entry_points_by_type, abi: None }).into())
}
