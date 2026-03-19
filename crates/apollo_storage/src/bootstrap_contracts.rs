//! Bootstrap contract artifacts and loader for the batcher bootstrap state machine.
//!
//! Loads the dummy account and ERC20 testing Sierra classes from resources and exposes them
//! (and their class hashes) for bootstrap initialization.

use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;

/// Path to the dummy account Sierra JSON (relative to crate manifest).
const DUMMY_ACCOUNT_SIERRA: &str =
    "resources/bootstrap_contracts/cairo1/sierra/dummy_account.sierra.json";
/// Path to the ERC20 testing Sierra JSON (relative to crate manifest).
const ERC20_TESTING_SIERRA: &str =
    "resources/bootstrap_contracts/cairo1/sierra/erc20_testing.sierra.json";

fn load_sierra(relative_path: &str) -> SierraContractClass {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    let contents = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("Failed to read bootstrap contract at {}: {}", path.display(), e)
    });
    let cairo_class: CairoLangContractClass =
        serde_json::from_str(&contents).expect("Invalid Sierra JSON");
    SierraContractClass::from(cairo_class)
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
