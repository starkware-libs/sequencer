use std::fs;
use std::path::Path;

use cairo_lang_starknet_classes::contract_class::{ContractClass, ContractEntryPoints};
use cairo_lang_utils::bigint::BigUintAsHex;
use serde::Deserialize;

/// Same as `ContractClass` - but ignores unnecessary fields like `abi` in deserialization.
#[derive(Deserialize)]
struct DeserializedContractClass {
    pub sierra_program: Vec<BigUintAsHex>,
    pub sierra_program_debug_info: Option<cairo_lang_sierra::debug_info::DebugInfo>,
    pub contract_class_version: String,
    pub entry_points_by_type: ContractEntryPoints,
}

pub fn contract_class_from_file<P: AsRef<Path>>(path: P) -> ContractClass {
    let DeserializedContractClass {
        sierra_program,
        sierra_program_debug_info,
        contract_class_version,
        entry_points_by_type,
    } = serde_json::from_str(&fs::read_to_string(path).expect("Failed to read input file."))
        .expect("deserialization Failed.");

    ContractClass {
        sierra_program,
        sierra_program_debug_info,
        contract_class_version,
        entry_points_by_type,
        abi: None,
    }
}
