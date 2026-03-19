//! Regression: `BootstrapLayout::EMBEDDED` must match derivation from embedded Sierra + address
//! rules.

use apollo_storage::bootstrap_contracts;
use starknet_api::core::{calculate_contract_address, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};

use crate::bootstrap::{derived_bootstrap_layout, BootstrapLayout, PRE_FEE_TOKEN_SETUP_NONCE};

#[test]
fn embedded_bootstrap_layout_matches_derived() {
    assert_eq!(BootstrapLayout::EMBEDDED, derived_bootstrap_layout());
}

#[test]
#[ignore]
fn print_derived_bootstrap_layout_hex() {
    let d = derived_bootstrap_layout();
    println!("account_class_hash: {}", d.account_class_hash.0.to_fixed_hex_string());
    println!("erc20_class_hash: {}", d.erc20_class_hash.0.to_fixed_hex_string());
    println!("account_address: {}", d.account_address.0.key().to_fixed_hex_string());
    println!("strk_address: {}", d.strk_address.0.key().to_fixed_hex_string());
}

#[test]
fn derived_layout_uses_same_rules_as_documented() {
    let account_class_hash = bootstrap_contracts::bootstrap_account_class_hash();
    let erc20_class_hash = bootstrap_contracts::bootstrap_erc20_class_hash();
    let account_address = calculate_contract_address(
        ContractAddressSalt::default(),
        account_class_hash,
        &Calldata::default(),
        ContractAddress::default(),
    )
    .expect("account address");
    let strk_deploy_nonce = Nonce(StarkHash::from(PRE_FEE_TOKEN_SETUP_NONCE));
    let strk_constructor_calldata = Calldata(vec![*account_address.0.key()].into());
    let strk_address = calculate_contract_address(
        ContractAddressSalt(strk_deploy_nonce.0),
        erc20_class_hash,
        &strk_constructor_calldata,
        account_address,
    )
    .expect("strk address");
    assert_eq!(
        derived_bootstrap_layout(),
        BootstrapLayout { account_class_hash, erc20_class_hash, account_address, strk_address }
    );
}
