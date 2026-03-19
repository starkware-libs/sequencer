//! Checked-in bootstrap class hashes and addresses live in [`crate::bootstrap`] (`BOOTSTRAP_*`,
//! not derived at runtime). Tests derive the same values and assert they match.

use apollo_storage::bootstrap_contracts::{
    bootstrap_account_class_hash,
    bootstrap_erc20_class_hash,
};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, calculate_contract_address};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};

use crate::bootstrap::{
    BOOTSTRAP_ACCOUNT_ADDRESS,
    BOOTSTRAP_ACCOUNT_CLASS_HASH,
    BOOTSTRAP_ERC20_CLASS_HASH,
    BOOTSTRAP_STRK_ADDRESS,
    PRE_FEE_TOKEN_SETUP_NONCE,
};

fn derived_bootstrap_values() -> (ClassHash, ClassHash, ContractAddress, ContractAddress) {
    let account_class_hash = bootstrap_account_class_hash();
    let erc20_class_hash = bootstrap_erc20_class_hash();

    let account_address = calculate_contract_address(
        ContractAddressSalt::default(),
        account_class_hash,
        &Calldata::default(),
        ContractAddress::default(),
    )
    .expect("Failed to calculate account contract address");

    let strk_deploy_nonce = Nonce(StarkHash::from(PRE_FEE_TOKEN_SETUP_NONCE));
    let strk_constructor_calldata = Calldata(vec![*account_address.0.key()].into());
    let strk_address = calculate_contract_address(
        ContractAddressSalt(strk_deploy_nonce.0),
        erc20_class_hash,
        &strk_constructor_calldata,
        account_address,
    )
    .expect("Failed to calculate STRK fee token contract address");

    (account_class_hash, erc20_class_hash, account_address, strk_address)
}

#[test]
fn non_derived_bootstrap_values_match_derived() {
    let (d_account_class, d_erc20, d_account_addr, d_strk) = derived_bootstrap_values();
    assert_eq!(BOOTSTRAP_ACCOUNT_CLASS_HASH, d_account_class);
    assert_eq!(BOOTSTRAP_ERC20_CLASS_HASH, d_erc20);
    assert_eq!(BOOTSTRAP_ACCOUNT_ADDRESS, d_account_addr);
    assert_eq!(BOOTSTRAP_STRK_ADDRESS, d_strk);
}

/// Manual helper: run with `cargo test -p apollo_batcher print_derived_bootstrap_values_hex --
/// --ignored` to print hex for updating the checked-in `BOOTSTRAP_*` consts when Sierra or deploy
/// rules change.
#[test]
#[ignore]
fn print_derived_bootstrap_values_hex() {
    let (account_class_hash, erc20_class_hash, account_address, strk_address) =
        derived_bootstrap_values();
    println!("account_class_hash: {}", account_class_hash.0.to_fixed_hex_string());
    println!("erc20_class_hash: {}", erc20_class_hash.0.to_fixed_hex_string());
    println!("account_address: {}", account_address.0.key().to_fixed_hex_string());
    println!("strk_address: {}", strk_address.0.key().to_fixed_hex_string());
}
