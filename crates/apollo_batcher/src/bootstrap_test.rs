use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::StorageReader;
use indexmap::IndexMap;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::block::BlockNumber;
use starknet_api::core::Nonce;
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_types_core::felt::Felt;

use crate::bootstrap::{BootstrapState, BootstrapStateMachine, BOOTSTRAP_SENDER_ADDRESS};

fn create_test_sm_and_storage()
-> (BootstrapStateMachine, StorageReader, apollo_storage::StorageWriter) {
    let sm = BootstrapStateMachine::new(true);
    let ((reader, writer), _temp_dir) = get_test_storage();
    std::mem::forget(_temp_dir);
    (sm, reader, writer)
}

fn declare_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
    ThinStateDiff {
        class_hash_to_compiled_class_hash: IndexMap::from([
            (sm.account_class_hash(), sm.account_compiled_class_hash()),
            (sm.erc20_class_hash(), sm.erc20_compiled_class_hash()),
        ]),
        ..Default::default()
    }
}

fn deploy_account_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: IndexMap::from([(sm.account_address(), sm.account_class_hash())]),
        nonces: IndexMap::from([(sm.account_address(), Nonce(StarkHash::from(1_u128)))]),
        ..Default::default()
    }
}

fn deploy_token_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: IndexMap::from([(sm.strk_address(), sm.erc20_class_hash())]),
        nonces: IndexMap::from([(sm.account_address(), Nonce(StarkHash::from(2_u128)))]),
        ..Default::default()
    }
}

fn fund_account_diff(sm: &BootstrapStateMachine) -> ThinStateDiff {
    let initialized_key = get_storage_var_address("initialized", &[]);
    ThinStateDiff {
        storage_diffs: IndexMap::from([(
            sm.strk_address(),
            IndexMap::from([(initialized_key, Felt::ONE)]),
        )]),
        nonces: IndexMap::from([(sm.account_address(), Nonce(StarkHash::from(3_u128)))]),
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
    let sm = BootstrapStateMachine::new(false);
    let ((reader, _writer), _temp_dir) = get_test_storage();
    assert_eq!(sm.current_state(&reader), BootstrapState::NotInBootstrap);
    assert!(sm.transactions_for_state(BootstrapState::NotInBootstrap).is_empty());
}

#[test]
fn empty_storage_returns_declare_contracts() {
    let (sm, reader, _writer) = create_test_sm_and_storage();
    assert_eq!(sm.current_state(&reader), BootstrapState::DeclareContracts);
}

#[test]
fn state_after_declare() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    append_diff(&mut writer, 0, declare_diff(&sm));
    assert_eq!(sm.current_state(&reader), BootstrapState::DeployAccount);
}

#[test]
fn state_after_deploy_account() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    append_diff(&mut writer, 0, declare_diff(&sm));
    append_diff(&mut writer, 1, deploy_account_diff(&sm));
    assert_eq!(sm.current_state(&reader), BootstrapState::DeployToken);
}

#[test]
fn state_after_deploy_token() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    append_diff(&mut writer, 0, declare_diff(&sm));
    append_diff(&mut writer, 1, deploy_account_diff(&sm));
    append_diff(&mut writer, 2, deploy_token_diff(&sm));
    assert_eq!(sm.current_state(&reader), BootstrapState::FundAccount);
}

#[test]
fn state_after_fund() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    append_diff(&mut writer, 0, declare_diff(&sm));
    append_diff(&mut writer, 1, deploy_account_diff(&sm));
    append_diff(&mut writer, 2, deploy_token_diff(&sm));
    append_diff(&mut writer, 3, fund_account_diff(&sm));
    assert_eq!(sm.current_state(&reader), BootstrapState::NotInBootstrap);
}

#[test]
#[should_panic(expected = "partial class declaration")]
fn partial_declaration_panics() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    let partial_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: IndexMap::from([(
            sm.account_class_hash,
            sm.account_compiled_class_hash,
        )]),
        ..Default::default()
    };
    append_diff(&mut writer, 0, partial_diff);
    sm.current_state(&reader);
}

#[test]
#[should_panic(expected = "deploy_token transaction may have reverted")]
fn deploy_token_revert_detected() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    append_diff(&mut writer, 0, declare_diff(&sm));
    let bad_diff = ThinStateDiff {
        deployed_contracts: IndexMap::from([(sm.account_address, sm.account_class_hash)]),
        nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(2_u128)))]),
        ..Default::default()
    };
    append_diff(&mut writer, 1, bad_diff);
    sm.current_state(&reader);
}

#[test]
#[should_panic(expected = "fund_account transaction may have reverted")]
fn fund_account_revert_detected() {
    let (sm, reader, mut writer) = create_test_sm_and_storage();
    append_diff(&mut writer, 0, declare_diff(&sm));
    append_diff(&mut writer, 1, deploy_account_diff(&sm));
    let bad_diff = ThinStateDiff {
        deployed_contracts: IndexMap::from([(sm.strk_address, sm.erc20_class_hash)]),
        nonces: IndexMap::from([(sm.account_address, Nonce(StarkHash::from(3_u128)))]),
        ..Default::default()
    };
    append_diff(&mut writer, 2, bad_diff);
    sm.current_state(&reader);
}

#[test]
fn declare_transactions_generated_correctly() {
    let sm = BootstrapStateMachine::new(true);
    let txs = sm.transactions_for_state(BootstrapState::DeclareContracts);

    assert_eq!(txs.len(), 2);
    assert!(matches!(txs[0], RpcTransaction::Declare(_)));
    assert!(matches!(txs[1], RpcTransaction::Declare(_)));

    let bootstrap_addr = starknet_api::core::ContractAddress::from(BOOTSTRAP_SENDER_ADDRESS);
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
    let sm = BootstrapStateMachine::new(true);
    let txs = sm.transactions_for_state(BootstrapState::DeployAccount);

    assert_eq!(txs.len(), 1);
    if let RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(ref deploy)) = txs[0] {
        assert_eq!(deploy.class_hash, sm.account_class_hash);
        assert_eq!(deploy.contract_address_salt, ContractAddressSalt::default());
        assert_eq!(deploy.nonce, Nonce::default());
    } else {
        panic!("Expected RpcDeployAccountTransaction::V3");
    }
}

#[test]
fn deploy_token_transaction_generated_correctly() {
    let sm = BootstrapStateMachine::new(true);
    let txs = sm.transactions_for_state(BootstrapState::DeployToken);

    assert_eq!(txs.len(), 1);
    if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(ref invoke)) = txs[0] {
        assert_eq!(invoke.sender_address, sm.account_address);
        assert_eq!(invoke.nonce, Nonce(StarkHash::from(1_u128)));
        assert!(!invoke.calldata.0.is_empty());
    } else {
        panic!("Expected RpcInvokeTransaction::V3");
    }
}

#[test]
fn fund_account_transaction_generated_correctly() {
    let sm = BootstrapStateMachine::new(true);
    let txs = sm.transactions_for_state(BootstrapState::FundAccount);

    assert_eq!(txs.len(), 1);
    if let RpcTransaction::Invoke(RpcInvokeTransaction::V3(ref invoke)) = txs[0] {
        assert_eq!(invoke.sender_address, sm.account_address);
        assert_eq!(invoke.nonce, Nonce(StarkHash::from(2_u128)));
        assert!(!invoke.calldata.0.is_empty());
    } else {
        panic!("Expected RpcInvokeTransaction::V3");
    }
}

#[test]
fn not_in_bootstrap_produces_no_transactions() {
    let sm = BootstrapStateMachine::new(true);
    assert!(sm.transactions_for_state(BootstrapState::NotInBootstrap).is_empty());
}

#[test]
fn deterministic_addresses_are_consistent() {
    let sm1 = BootstrapStateMachine::new(true);
    let sm2 = BootstrapStateMachine::new(true);

    assert_eq!(sm1.account_address(), sm2.account_address());
    assert_eq!(sm1.strk_address(), sm2.strk_address());
    assert_ne!(sm1.account_address(), starknet_api::core::ContractAddress::default());
    assert_ne!(sm1.strk_address(), starknet_api::core::ContractAddress::default());
}
