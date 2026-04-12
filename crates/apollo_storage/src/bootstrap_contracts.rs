//! Embedded Sierra/CASM artifacts for bootstrapping Starknet node storage.
//!
//! Dummy account and ERC20 testing Sierra/CASM JSON is compiled into the binary via
//! `include_str!`, so bootstrap works for a shipped executable without a checkout or resource
//! directory on disk.

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use serde_json::from_str;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::rpc_transaction::EntryPointByType;
use starknet_api::state::SierraContractClass;
use starknet_types_core::felt::Felt;

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

/// Hex class hash of the embedded bootstrap dummy account Sierra artifact
/// (`bootstrap_account_class_hash`).
pub const BOOTSTRAP_ACCOUNT_CLASS_HASH: &str =
    "0x23f6d63bd54a867e571beb1f98b5461f7f58b7647c01b2b4fb4b00c157bc709";
/// Hex class hash of the embedded bootstrap ERC20 testing Sierra artifact
/// (`bootstrap_erc20_class_hash`).
pub const BOOTSTRAP_ERC20_CLASS_HASH: &str =
    "0x2cde22a0f2c81295709bbe71ba6a1cf53283720e91b1f4cd11ca42c879f4402";
/// Hex compiled class hash (V2) of the embedded bootstrap account CASM
/// (`bootstrap_account_compiled_class_hash`).
pub const BOOTSTRAP_ACCOUNT_COMPILED_CLASS_HASH: &str =
    "0x1a4828d73b49e6ec515d2c879a5a1b2870439c83c81517e40973d8f2d11b1a7";
/// Hex compiled class hash (V2) of the embedded bootstrap ERC20 CASM
/// (`bootstrap_erc20_compiled_class_hash`).
pub const BOOTSTRAP_ERC20_COMPILED_CLASS_HASH: &str =
    "0x7352cd4c7c86d16bb9dbe28d286f78279e27017f731f8afe1562dede8a41cb3";

/// Converts cairo_lang ContractClass to starknet_api SierraContractClass.
/// We do this locally because the From impl in starknet_api is only available with the "testing"
/// feature; apollo_storage uses bootstrap in production without that feature.
fn cairo_contract_class_to_sierra(cairo_class: CairoLangContractClass) -> SierraContractClass {
    SierraContractClass {
        sierra_program: cairo_class
            .sierra_program
            .into_iter()
            .map(|big_uint_as_hex| Felt::from(big_uint_as_hex.value))
            .collect(),
        contract_class_version: cairo_class.contract_class_version,
        entry_points_by_type: EntryPointByType::from(cairo_class.entry_points_by_type),
        abi: cairo_class
            .abi
            .map(|abi| serde_json::to_string(&abi).expect("ABI is valid JSON"))
            .unwrap_or_default(),
    }
}

fn sierra_from_json(json: &str) -> SierraContractClass {
    let cairo_class: CairoLangContractClass = from_str(json).expect("Invalid Sierra JSON");
    cairo_contract_class_to_sierra(cairo_class)
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
