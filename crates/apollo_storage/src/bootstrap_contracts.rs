//! Hardcoded Sierra/CASM JSON for bootstrapping Starknet node storage.
//!
//! Dummy account and ERC20 testing artifacts are compiled into the binary via `include_str!`, so
//! bootstrap works for a shipped executable without a checkout or resource directory on disk.

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use serde_json::from_str;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::state::SierraContractClass;

#[cfg(test)]
#[path = "bootstrap_contracts_test.rs"]
mod bootstrap_contracts_test;

const DUMMY_ACCOUNT_SIERRA_JSON: &str =
    include_str!("../resources/bootstrap_contracts/cairo1/sierra/dummy_account.sierra.json");
const ERC20_TESTING_SIERRA_JSON: &str =
    include_str!("../resources/bootstrap_contracts/cairo1/sierra/erc20_testing.sierra.json");
const DUMMY_ACCOUNT_CASM_JSON: &str =
    include_str!("../resources/bootstrap_contracts/cairo1/compiled/dummy_account.casm.json");
const ERC20_TESTING_CASM_JSON: &str =
    include_str!("../resources/bootstrap_contracts/cairo1/compiled/erc20_testing.casm.json");

/// Hex class hash of the hardcoded bootstrap dummy account Sierra artifact
/// (`bootstrap_account_class_hash`).
pub const BOOTSTRAP_ACCOUNT_CLASS_HASH: &str =
    "0x36584049d6c9c6e961ae9d87d69006f1379637b8093063b46720e499bbeb9dd";
/// Hex class hash of the hardcoded bootstrap ERC20 testing Sierra artifact
/// (`bootstrap_erc20_class_hash`).
pub const BOOTSTRAP_ERC20_CLASS_HASH: &str =
    "0x72c19ab0a7d7a46250611ebf7af7bfcfe1a330d60bee3e4a7784c56be043983";
/// Hex compiled class hash (V2) of the hardcoded bootstrap account CASM
/// (`bootstrap_account_compiled_class_hash`).
pub const BOOTSTRAP_ACCOUNT_COMPILED_CLASS_HASH: &str =
    "0x1a4828d73b49e6ec515d2c879a5a1b2870439c83c81517e40973d8f2d11b1a7";
/// Hex compiled class hash (V2) of the hardcoded bootstrap ERC20 CASM
/// (`bootstrap_erc20_compiled_class_hash`).
pub const BOOTSTRAP_ERC20_COMPILED_CLASS_HASH: &str =
    "0x7352cd4c7c86d16bb9dbe28d286f78279e27017f731f8afe1562dede8a41cb3";

fn sierra_from_json(json: &str) -> SierraContractClass {
    let cairo_class: CairoLangContractClass = from_str(json).expect("Invalid Sierra JSON");
    SierraContractClass::from(cairo_class)
}

fn casm_from_json(json: &str) -> CasmContractClass {
    from_str(json).expect("Invalid CASM JSON")
}

/// Returns the Sierra contract class for the bootstrap account (dummy account).
pub fn bootstrap_account_sierra() -> SierraContractClass {
    sierra_from_json(DUMMY_ACCOUNT_SIERRA_JSON)
}

/// Returns the Sierra contract class for the bootstrap ERC20 fee token.
pub fn bootstrap_erc20_sierra() -> SierraContractClass {
    sierra_from_json(ERC20_TESTING_SIERRA_JSON)
}

/// Returns the class hash of the bootstrap account contract.
pub fn bootstrap_account_class_hash() -> ClassHash {
    bootstrap_account_sierra().calculate_class_hash()
}

/// Returns the class hash of the bootstrap ERC20 contract.
pub fn bootstrap_erc20_class_hash() -> ClassHash {
    bootstrap_erc20_sierra().calculate_class_hash()
}

/// Returns the compiled class hash (V2) of the bootstrap account contract.
pub fn bootstrap_account_compiled_class_hash() -> CompiledClassHash {
    casm_from_json(DUMMY_ACCOUNT_CASM_JSON).hash(&HashVersion::V2)
}

/// Returns the compiled class hash (V2) of the bootstrap ERC20 contract.
pub fn bootstrap_erc20_compiled_class_hash() -> CompiledClassHash {
    casm_from_json(ERC20_TESTING_CASM_JSON).hash(&HashVersion::V2)
}
