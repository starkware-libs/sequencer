use std::collections::BTreeSet;

use apollo_batcher_config::config::StorageAccessFilterConfig;
use blockifier::execution::call_info::{CallInfo, StorageAccessTracker};
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction;
use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::{invoke_tx_args, storage_key};

use crate::storage_access_filter::create_storage_access_filter;

const BLOCKED_STORAGE_KEY: u8 = 0x10;
const EXEMPT_ACCOUNT_ADDRESS: u8 = 0x20;

fn call_info(accessed_storage_key: u8, inner_calls: Vec<CallInfo>) -> CallInfo {
    CallInfo {
        storage_access_tracker: StorageAccessTracker {
            accessed_storage_keys: [storage_key!(accessed_storage_key)].into(),
            ..Default::default()
        },
        inner_calls,
        ..Default::default()
    }
}

#[rstest]
#[case::nested_inner_call(
    None,
    Some(call_info(0x1, vec![call_info(0x2, vec![call_info(BLOCKED_STORAGE_KEY, vec![])])])),
    0x1,
    true
)]
#[case::validate_call(Some(call_info(BLOCKED_STORAGE_KEY, vec![])), None, 0x1, true)]
#[case::other_keys(Some(call_info(0x1, vec![])), Some(call_info(0x11, vec![])), 0x1, false)]
#[case::exempt_account(None, Some(call_info(BLOCKED_STORAGE_KEY, vec![])), EXEMPT_ACCOUNT_ADDRESS, false)]
fn test_storage_access_filter(
    #[case] validate_call_info: Option<CallInfo>,
    #[case] execute_call_info: Option<CallInfo>,
    #[case] sender_address: u8,
    #[case] expect_rejected: bool,
) {
    let config = StorageAccessFilterConfig {
        blocked_storage_keys: BTreeSet::from([StorageKey::from(BLOCKED_STORAGE_KEY)]),
        exempt_account_addresses: BTreeSet::from([ContractAddress::from(EXEMPT_ACCOUNT_ADDRESS)]),
        ..Default::default()
    };
    let tx = Transaction::Account(AccountTransaction::new_for_sequencing(executable_invoke_tx(
        invoke_tx_args!(sender_address: ContractAddress::from(sender_address)),
    )));
    let tx_execution_info =
        TransactionExecutionInfo { validate_call_info, execute_call_info, ..Default::default() };

    let result = create_storage_access_filter(&config).unwrap().check(&tx, &tx_execution_info);
    assert_eq!(result.is_err(), expect_rejected, "{result:?}");
}
