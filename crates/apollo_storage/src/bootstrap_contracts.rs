//! Bootstrap contract artifacts and loader for the batcher bootstrap state machine.
//!
//! Loads the dummy account and ERC20 testing Sierra classes from resources and exposes them
//! (and their class hashes and compiled class hashes) for bootstrap initialization.

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::rpc_transaction::EntryPointByType;
use starknet_api::state::SierraContractClass;
use starknet_types_core::felt::Felt;

/// Path to the dummy account Sierra JSON (relative to crate manifest).
const DUMMY_ACCOUNT_SIERRA: &str =
    "resources/bootstrap_contracts/cairo1/sierra/dummy_account.sierra.json";
/// Path to the ERC20 testing Sierra JSON (relative to crate manifest).
const ERC20_TESTING_SIERRA: &str =
    "resources/bootstrap_contracts/cairo1/sierra/erc20_testing.sierra.json";
/// Path to the dummy account CASM JSON (relative to crate manifest).
const DUMMY_ACCOUNT_CASM: &str =
    "resources/bootstrap_contracts/cairo1/compiled/dummy_account.casm.json";
/// Path to the ERC20 testing CASM JSON (relative to crate manifest).
const ERC20_TESTING_CASM: &str =
    "resources/bootstrap_contracts/cairo1/compiled/erc20_testing.casm.json";

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

fn load_sierra(relative_path: &str) -> SierraContractClass {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    let contents = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("Failed to read bootstrap contract at {}: {}", path.display(), e)
    });
    let cairo_class: CairoLangContractClass =
        serde_json::from_str(&contents).expect("Invalid Sierra JSON");
    cairo_contract_class_to_sierra(cairo_class)
}

/// Returns the Sierra contract class for the bootstrap account (dummy account).
pub fn bootstrap_account_sierra() -> SierraContractClass {
    load_sierra(DUMMY_ACCOUNT_SIERRA)
}

/// Returns the Sierra contract class for the bootstrap ERC20 fee token.
pub fn bootstrap_erc20_sierra() -> SierraContractClass {
    load_sierra(ERC20_TESTING_SIERRA)
}

/// Returns the class hash of the bootstrap account contract.
pub fn bootstrap_account_class_hash() -> ClassHash {
    bootstrap_account_sierra().calculate_class_hash()
}

/// Returns the class hash of the bootstrap ERC20 contract.
pub fn bootstrap_erc20_class_hash() -> ClassHash {
    bootstrap_erc20_sierra().calculate_class_hash()
}

fn load_casm(relative_path: &str) -> CasmContractClass {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read bootstrap CASM at {}: {}", path.display(), e));
    serde_json::from_str(&contents).expect("Invalid CASM JSON")
}

/// Returns the compiled class hash (V2) of the bootstrap account contract.
pub fn bootstrap_account_compiled_class_hash() -> CompiledClassHash {
    load_casm(DUMMY_ACCOUNT_CASM).hash(&HashVersion::V2)
}

/// Returns the compiled class hash (V2) of the bootstrap ERC20 contract.
pub fn bootstrap_erc20_compiled_class_hash() -> CompiledClassHash {
    load_casm(ERC20_TESTING_CASM).hash(&HashVersion::V2)
}
