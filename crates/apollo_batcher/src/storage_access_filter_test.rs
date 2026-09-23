use std::collections::BTreeSet;

use apollo_batcher_config::config::StorageAccessFilterConfig;
use assert_matches::assert_matches;
use blockifier::execution::call_info::{CallInfo, StorageAccessTracker};
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::errors::TransactionExecutionError;
use blockifier::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use blockifier::transaction::transaction_execution::Transaction;
use rstest::rstest;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::{invoke_tx_args, storage_key};

use crate::storage_access_filter::create_storage_access_filter;

const ERROR_MESSAGE: &str = "Blocked.";

fn blocked_storage_key() -> StorageKey {
    storage_key!(0x10_u8)
}

fn call_info(accessed_storage_keys: &[StorageKey], inner_calls: Vec<CallInfo>) -> CallInfo {
    CallInfo {
        storage_access_tracker: StorageAccessTracker {
            accessed_storage_keys: accessed_storage_keys.iter().copied().collect(),
            ..Default::default()
        },
        inner_calls,
        ..Default::default()
    }
}

fn check(tx_execution_info: TransactionExecutionInfo) -> TransactionExecutionResult<()> {
    let config = StorageAccessFilterConfig {
        blocked_storage_keys: BTreeSet::from([blocked_storage_key()]),
        error_message: ERROR_MESSAGE.to_string(),
    };
    let tx = Transaction::Account(AccountTransaction::new_for_sequencing(executable_invoke_tx(
        invoke_tx_args!(),
    )));
    create_storage_access_filter(&config).unwrap().check(&tx, &tx_execution_info)
}

#[rstest]
#[case::execute_call(TransactionExecutionInfo {
    execute_call_info: Some(call_info(&[storage_key!(0x1_u8), blocked_storage_key()], vec![])),
    ..Default::default()
})]
#[case::nested_inner_call(TransactionExecutionInfo {
    execute_call_info: Some(call_info(
        &[storage_key!(0x1_u8)],
        vec![call_info(&[], vec![call_info(&[blocked_storage_key()], vec![])])],
    )),
    ..Default::default()
})]
#[case::validate_call(TransactionExecutionInfo {
    validate_call_info: Some(call_info(&[blocked_storage_key()], vec![])),
    ..Default::default()
})]
fn test_blocked_storage_key_access_is_rejected(
    #[case] tx_execution_info: TransactionExecutionInfo,
) {
    assert_matches!(
        check(tx_execution_info),
        Err(TransactionExecutionError::RejectedByTransactionFilter { message })
            if message == ERROR_MESSAGE
    );
}

#[test]
fn test_unblocked_storage_key_access_is_allowed() {
    let tx_execution_info = TransactionExecutionInfo {
        validate_call_info: Some(call_info(&[storage_key!(0x1_u8)], vec![])),
        execute_call_info: Some(call_info(
            &[storage_key!(0x11_u8)],
            vec![call_info(&[storage_key!(0x0_u8)], vec![])],
        )),
        ..Default::default()
    };
    assert_matches!(check(tx_execution_info), Ok(()));
}

#[test]
fn test_no_filter_without_blocked_storage_keys() {
    assert!(create_storage_access_filter(&StorageAccessFilterConfig::default()).is_none());
}
