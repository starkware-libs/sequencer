// This module contains code taken from starknet-replay.
// For more information, see the original repository at:
// `<starknet-replay: https://github.com/lambdaclass/starknet-replay>`

use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::LazyLock;

use apollo_compile_to_casm::{create_sierra_compiler, SierraCompiler};
use apollo_compile_to_casm_types::RawClass;
use apollo_gateway::errors::serde_err_to_state_err;
use apollo_sierra_compilation_config::config::SierraCompilationConfig;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateResult;
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
    FlattenedSierraClass,
    LegacyContractEntryPoint,
    LegacyEntryPointsByType,
};

static SIERRA_COMPILER: LazyLock<SierraCompiler> =
    LazyLock::new(|| create_sierra_compiler(SierraCompilationConfig::default()));

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

/// Compile a FlattenedSierraClass to a versioned ContractClass V1 (casm) using
/// cairo_lang_starknet_classes.
pub fn sierra_to_versioned_contract_class_v1(
    sierra: FlattenedSierraClass,
) -> StateResult<(ContractClass, SierraVersion)> {
    let serde_value = serde_json::to_value(&sierra).map_err(serde_err_to_state_err)?;
    let sierra_contract: SierraContractClass =
        serde_json::from_value(serde_value).map_err(serde_err_to_state_err)?;
    let sierra_version = SierraVersion::extract_from_program(&sierra_contract.sierra_program)
        .map_err(StateError::from)?;
    let raw_class = RawClass::try_from(sierra_contract.clone()).map_err(serde_err_to_state_err)?;
    let (raw_executable_class, _) = SIERRA_COMPILER
        .compile(raw_class)
        .map_err(|err| StateError::StateReadError(format!("Failed to compile Sierra: {err}")))?;
    let contract_class: ContractClass = serde_json::from_value(raw_executable_class.into_value())
        .map_err(serde_err_to_state_err)?;

    Ok((contract_class, sierra_version))
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
