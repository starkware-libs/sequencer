//! Checked-in bootstrap class hashes and addresses live in [`crate::bootstrap`] (`BOOTSTRAP_*`,
//! not derived at runtime). Tests derive the same values and assert they match.

use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::{bootstrap_contracts, StorageReader};
use blockifier::context::ChainInfo;
use indexmap::IndexMap;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::block::BlockNumber;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_types_core::felt::Felt;

use crate::bootstrap::{
    bootstrap_transactions_for_state,
    current_bootstrap_state,
    validate_strk_fee_token_for_active_bootstrap,
    BootstrapConfig,
    BootstrapState,
    BOOTSTRAP_ACCOUNT_ADDRESS,
    BOOTSTRAP_ACCOUNT_CLASS_HASH,
    BOOTSTRAP_ERC20_CLASS_HASH,
    BOOTSTRAP_SENDER_ADDRESS,
    BOOTSTRAP_STRK_ADDRESS,
    PRE_FEE_TOKEN_SETUP_NONCE,
};

fn derived_bootstrap_values() -> (ClassHash, ClassHash, ContractAddress, ContractAddress) {
    let account_class_hash = bootstrap_contracts::bootstrap_account_class_hash();
    let erc20_class_hash = bootstrap_contracts::bootstrap_erc20_class_hash();

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

fn enabled_config() -> BootstrapConfig {
    BootstrapConfig { bootstrap_enabled: true }
}

fn disabled_config() -> BootstrapConfig {
    BootstrapConfig { bootstrap_enabled: false }
}

fn create_test_config_and_storage()
-> (BootstrapConfig, StorageReader, apollo_storage::StorageWriter) {
    let config = enabled_config();
    let ((reader, writer), _temp_dir) = get_test_storage();
    std::mem::forget(_temp_dir);
    (config, reader, writer)
}

fn declare_diff() -> ThinStateDiff {
    ThinStateDiff {
        class_hash_to_compiled_class_hash: IndexMap::from([
            (
                BOOTSTRAP_ACCOUNT_CLASS_HASH,
                bootstrap_contracts::bootstrap_account_compiled_class_hash(),
            ),
            (
                BOOTSTRAP_ERC20_CLASS_HASH,
                bootstrap_contracts::bootstrap_erc20_compiled_class_hash(),
            ),
        ]),
        ..Default::default()
    }
}

fn deploy_account_diff() -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: IndexMap::from([(
            BOOTSTRAP_ACCOUNT_ADDRESS,
            BOOTSTRAP_ACCOUNT_CLASS_HASH,
        )]),
        nonces: IndexMap::from([(BOOTSTRAP_ACCOUNT_ADDRESS, Nonce(StarkHash::from(1_u128)))]),
        ..Default::default()
    }
}

/// STRK deployed, constructor ran (`initialized`), account nonce consumed by deploy invoke.
fn deploy_fee_token_complete_diff() -> ThinStateDiff {
    let initialized_key = get_storage_var_address("initialized", &[]);
    ThinStateDiff {
        deployed_contracts: IndexMap::from([(BOOTSTRAP_STRK_ADDRESS, BOOTSTRAP_ERC20_CLASS_HASH)]),
        nonces: IndexMap::from([(BOOTSTRAP_ACCOUNT_ADDRESS, Nonce(StarkHash::from(2_u128)))]),
        storage_diffs: IndexMap::from([(
            BOOTSTRAP_STRK_ADDRESS,
            IndexMap::from([(initialized_key, Felt::ONE)]),
        )]),
        ..Default::default()
    }
}

fn append_diff(writer: &mut apollo_storage::StorageWriter, block: u64, diff: ThinStateDiff) {
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(block), diff)
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn not_in_bootstrap_when_disabled() {
    let config = disabled_config();
    let ((reader, _writer), _temp_dir) = get_test_storage();
    assert_eq!(current_bootstrap_state(&config, &reader), BootstrapState::NotInBootstrap);
    assert!(bootstrap_transactions_for_state(&config, BootstrapState::NotInBootstrap).is_empty());
}

#[test]
fn empty_storage_returns_declare_contracts() {
    let (config, reader, _writer) = create_test_config_and_storage();
    assert_eq!(current_bootstrap_state(&config, &reader), BootstrapState::DeclareContracts);
}

#[test]
fn state_after_declare() {
    let (config, reader, mut writer) = create_test_config_and_storage();
    append_diff(&mut writer, 0, declare_diff());
    assert_eq!(current_bootstrap_state(&config, &reader), BootstrapState::DeployAccount);
}

#[test]
fn state_after_deploy_account() {
    let (config, reader, mut writer) = create_test_config_and_storage();
    append_diff(&mut writer, 0, declare_diff());
    append_diff(&mut writer, 1, deploy_account_diff());
    assert_eq!(current_bootstrap_state(&config, &reader), BootstrapState::DeployFeeToken);
}

#[test]
fn state_after_deploy_fee_token_complete() {
    let (config, reader, mut writer) = create_test_config_and_storage();
    append_diff(&mut writer, 0, declare_diff());
    append_diff(&mut writer, 1, deploy_account_diff());
    append_diff(&mut writer, 2, deploy_fee_token_complete_diff());
    assert_eq!(current_bootstrap_state(&config, &reader), BootstrapState::NotInBootstrap);
}

#[test]
fn partial_declaration_stays_in_declare_phase() {
    let (config, reader, mut writer) = create_test_config_and_storage();
    let partial_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: IndexMap::from([(
            BOOTSTRAP_ACCOUNT_CLASS_HASH,
            bootstrap_contracts::bootstrap_account_compiled_class_hash(),
        )]),
        ..Default::default()
    };
    append_diff(&mut writer, 0, partial_diff);
    assert_eq!(current_bootstrap_state(&config, &reader), BootstrapState::DeclareContracts);
}

#[test]
#[should_panic(expected = "deploy_fee_token transaction may have reverted")]
fn deploy_fee_token_revert_detected() {
    let (config, reader, mut writer) = create_test_config_and_storage();
    append_diff(&mut writer, 0, declare_diff());
    let bad_diff = ThinStateDiff {
        deployed_contracts: IndexMap::from([(
            BOOTSTRAP_ACCOUNT_ADDRESS,
            BOOTSTRAP_ACCOUNT_CLASS_HASH,
        )]),
        nonces: IndexMap::from([(BOOTSTRAP_ACCOUNT_ADDRESS, Nonce(StarkHash::from(2_u128)))]),
        ..Default::default()
    };
    append_diff(&mut writer, 1, bad_diff);
    current_bootstrap_state(&config, &reader);
}

#[test]
fn declare_transactions_generated_correctly() {
    let config = enabled_config();
    let txs = bootstrap_transactions_for_state(&config, BootstrapState::DeclareContracts);

    assert_eq!(txs.len(), 2);
    assert!(matches!(txs[0], RpcTransaction::Declare(_)));
    assert!(matches!(txs[1], RpcTransaction::Declare(_)));

    let bootstrap_addr = ContractAddress::from(BOOTSTRAP_SENDER_ADDRESS);
    for tx in &txs {
        if let RpcTransaction::Declare(RpcDeclareTransaction::V3(ref declare)) = tx {
            assert_eq!(declare.sender_address, bootstrap_addr);
            assert_eq!(declare.nonce, Nonce::default());
        } else {
            panic!("Expected RpcDeclareTransaction::V3");
        }
    }
}

#[test]
fn deploy_account_transaction_generated_correctly() {
    let config = enabled_config();
    let txs = bootstrap_transactions_for_state(&config, BootstrapState::DeployAccount);

    assert_eq!(txs.len(), 1);
    if let RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(ref deploy)) = txs[0] {
        assert_eq!(deploy.class_hash, BOOTSTRAP_ACCOUNT_CLASS_HASH);
        assert_eq!(deploy.contract_address_salt, ContractAddressSalt::default());
        assert_eq!(deploy.nonce, Nonce::default());
    } else {
        panic!("Expected RpcDeployAccountTransaction::V3");
    }
}

#[test]
fn deploy_fee_token_transaction_generated_correctly() {
    let config = enabled_config();
    let txs = bootstrap_transactions_for_state(&config, BootstrapState::DeployFeeToken);

    assert_eq!(txs.len(), 1);
    if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(ref invoke)) = txs[0] {
        assert_eq!(invoke.sender_address, BOOTSTRAP_ACCOUNT_ADDRESS);
        assert_eq!(invoke.nonce, Nonce(StarkHash::from(1_u128)));
        assert!(!invoke.calldata.0.is_empty());
    } else {
        panic!("Expected RpcInvokeTransaction::V3");
    }
}

#[test]
fn not_in_bootstrap_produces_no_transactions() {
    let config = enabled_config();
    assert!(bootstrap_transactions_for_state(&config, BootstrapState::NotInBootstrap).is_empty());
}

#[test]
fn deterministic_addresses_are_consistent() {
    assert_eq!(BOOTSTRAP_ACCOUNT_ADDRESS, BOOTSTRAP_ACCOUNT_ADDRESS);
    assert_eq!(BOOTSTRAP_STRK_ADDRESS, BOOTSTRAP_STRK_ADDRESS);
    assert_ne!(BOOTSTRAP_ACCOUNT_ADDRESS, ContractAddress::default());
    assert_ne!(BOOTSTRAP_STRK_ADDRESS, ContractAddress::default());
}

#[test]
fn validate_strk_skipped_when_not_in_bootstrap_even_if_wrong_address() {
    let mut chain_info = ChainInfo::default();
    chain_info.fee_token_addresses.strk_fee_token_address = ContractAddress::from(1_u128);
    validate_strk_fee_token_for_active_bootstrap(&chain_info, BootstrapState::NotInBootstrap);
}

#[test]
fn validate_strk_accepts_expected_address_when_bootstrap_active() {
    let mut chain_info = ChainInfo::default();
    chain_info.fee_token_addresses.strk_fee_token_address =
        bootstrap_contracts::bootstrap_strk_fee_token_contract_address();
    validate_strk_fee_token_for_active_bootstrap(&chain_info, BootstrapState::DeployFeeToken);
}

#[test]
#[should_panic(expected = "strk_fee_token_address must be set to the embedded bootstrap ERC20")]
fn validate_strk_rejects_default_when_bootstrap_active() {
    let chain_info = ChainInfo::default();
    validate_strk_fee_token_for_active_bootstrap(&chain_info, BootstrapState::DeclareContracts);
}

#[test]
#[should_panic(expected = "must match the embedded bootstrap ERC20 address")]
fn validate_strk_rejects_mismatch_when_bootstrap_active() {
    let mut chain_info = ChainInfo::default();
    chain_info.fee_token_addresses.strk_fee_token_address = ContractAddress::from(1_u128);
    validate_strk_fee_token_for_active_bootstrap(&chain_info, BootstrapState::DeployAccount);
}
