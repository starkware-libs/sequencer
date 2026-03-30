//! Hardcoded Sierra/CASM JSON for bootstrapping Starknet node storage.
//!
//! Dummy account and ERC20 testing artifacts are compiled into the binary via `include_str!`, so
//! bootstrap works for a shipped executable without a checkout or resource directory on disk.

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use serde_json::from_str;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    calculate_contract_address,
};
use starknet_api::hash::StarkHash;
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};

#[cfg(test)]
#[path = "bootstrap_contracts_test.rs"]
mod bootstrap_contracts_test;

/// Account nonce after `deploy_account`; used as salt for STRK deploy (must stay in sync with
/// `PRE_FEE_TOKEN_SETUP_NONCE` in the batcher bootstrap module).
const PRE_FEE_TOKEN_SETUP_NONCE_U128: u128 = 1;

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
/// Hex address of the bootstrap STRK fee token (`bootstrap_strk_fee_token_contract_address`).
/// Must match `BOOTSTRAP_STRK_ADDRESS` in the batcher bootstrap module.
pub const BOOTSTRAP_STRK_FEE_TOKEN_ADDRESS: &str =
    "0x00147c72fb4b344340f0ea15948cd438ad45caac3c8e8428b1e3493b13d41f00";

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

/// Deterministic address of the bootstrap account (`deploy_account`: salt 0, empty calldata).
pub fn bootstrap_account_contract_address() -> ContractAddress {
    let account_class_hash = bootstrap_account_class_hash();
    calculate_contract_address(
        ContractAddressSalt::default(),
        account_class_hash,
        &Calldata::default(),
        ContractAddress::default(),
    )
    .expect("Failed to calculate account contract address")
}

/// Returns the compiled class hash (V2) of the bootstrap account contract.
pub fn bootstrap_account_compiled_class_hash() -> CompiledClassHash {
    casm_from_json(DUMMY_ACCOUNT_CASM_JSON).hash(&HashVersion::V2)
}

/// Returns the compiled class hash (V2) of the bootstrap ERC20 contract.
pub fn bootstrap_erc20_compiled_class_hash() -> CompiledClassHash {
    casm_from_json(ERC20_TESTING_CASM_JSON).hash(&HashVersion::V2)
}

/// Deterministic address of the STRK fee token deployed during bootstrap (ERC20 from hardcoded
/// Sierra; same `calculate_contract_address` rules as the batcher `BootstrapLayout`).
pub fn bootstrap_strk_fee_token_contract_address() -> ContractAddress {
    let erc20_class_hash = bootstrap_erc20_class_hash();
    let account_address = bootstrap_account_contract_address();

    let strk_deploy_nonce = Nonce(StarkHash::from(PRE_FEE_TOKEN_SETUP_NONCE_U128));
    let strk_constructor_calldata = Calldata(vec![*account_address.0.key()].into());
    calculate_contract_address(
        ContractAddressSalt(strk_deploy_nonce.0),
        erc20_class_hash,
        &strk_constructor_calldata,
        account_address,
    )
    .expect("Failed to calculate STRK fee token contract address")
}
