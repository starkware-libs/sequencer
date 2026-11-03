use std::collections::BTreeSet;

use apollo_batcher_config::config::{BatcherDynamicConfig, StorageAccessFilterConfig};
use blockifier::execution::call_info::{CallExecution, CallInfo, Retdata, StorageAccessTracker};
use blockifier::execution::contract_class::TrackedResource;
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::constants::VALIDATE_RETDATA;
use starknet_api::{felt, invoke_tx_args, storage_key};
use starknet_types_core::felt::Felt;

use crate::storage_access_filter::{create_storage_access_filter, create_transaction_filter};

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
    None,
    0x1,
    true
)]
#[case::validate_call(Some(call_info(BLOCKED_STORAGE_KEY, vec![])), None, None, 0x1, true)]
#[case::fee_transfer_call(None, None, Some(call_info(BLOCKED_STORAGE_KEY, vec![])), 0x1, true)]
#[case::other_keys(
    Some(call_info(0x1, vec![])),
    Some(call_info(0x11, vec![])),
    Some(call_info(0x12, vec![])),
    0x1,
    false
)]
#[case::exempt_account(
    None,
    Some(call_info(BLOCKED_STORAGE_KEY, vec![])),
    None,
    EXEMPT_ACCOUNT_ADDRESS,
    false
)]
fn test_storage_access_filter(
    #[case] validate_call_info: Option<CallInfo>,
    #[case] execute_call_info: Option<CallInfo>,
    #[case] fee_transfer_call_info: Option<CallInfo>,
    #[case] sender_address: u8,
    #[case] expect_rejected: bool,
) {
    let config = StorageAccessFilterConfig {
        blocked_storage_keys: BTreeSet::from([StorageKey::from(BLOCKED_STORAGE_KEY)]),
        exempt_account_address: Some(ContractAddress::from(EXEMPT_ACCOUNT_ADDRESS)),
    };
    let tx = Transaction::Account(AccountTransaction::new_for_sequencing(executable_invoke_tx(
        invoke_tx_args!(sender_address: ContractAddress::from(sender_address)),
    )));
    let tx_execution_info = TransactionExecutionInfo {
        validate_call_info,
        execute_call_info,
        fee_transfer_call_info,
        ..Default::default()
    };

    let result = create_storage_access_filter(&config).unwrap().check(&tx, &tx_execution_info);
    assert_eq!(result.is_err(), expect_rejected, "{result:?}");
}

fn sig_call(entry_point_name: &str, retdata: Vec<Felt>, failed: bool) -> CallInfo {
    CallInfo {
        call: CallEntryPoint {
            entry_point_selector: selector_from_name(entry_point_name),
            ..Default::default()
        },
        execution: CallExecution { retdata: Retdata(retdata), failed, ..Default::default() },
        tracked_resource: TrackedResource::CairoSteps,
        ..Default::default()
    }
}

#[rstest]
#[case::cairo_steps_snake_zero_rejected(
    TrackedResource::CairoSteps,
    "is_valid_signature",
    vec![felt!(0_u8)],
    false,
    true
)]
#[case::cairo_steps_camel_zero_rejected(
    TrackedResource::CairoSteps,
    "isValidSignature",
    vec![felt!(0_u8)],
    false,
    true
)]
#[case::cairo_steps_unexpected_felt_rejected(
    TrackedResource::CairoSteps,
    "is_valid_signature",
    vec![felt!(2_u8)],
    false,
    true
)]
#[case::cairo_steps_valid_magic_accepted(
    TrackedResource::CairoSteps,
    "is_valid_signature",
    vec![*VALIDATE_RETDATA],
    false,
    false
)]
#[case::cairo_steps_one_accepted(
    TrackedResource::CairoSteps,
    "isValidSignature",
    vec![felt!(1_u8)],
    false,
    false
)]
#[case::cairo_steps_empty_cairo0_accepted(
    TrackedResource::CairoSteps,
    "is_valid_signature",
    vec![],
    false,
    false
)]
#[case::cairo_steps_reverted_inner_call_accepted(
    TrackedResource::CairoSteps,
    "is_valid_signature",
    vec![felt!(0_u8)],
    true,
    false
)]
#[case::sierra_gas_caller_not_rejected(
    TrackedResource::SierraGas,
    "is_valid_signature",
    vec![felt!(0_u8)],
    false,
    false
)]
#[case::unrelated_selector_accepted(
    TrackedResource::CairoSteps,
    "transfer",
    vec![felt!(0_u8)],
    false,
    false
)]
fn test_signature_return_data_filter(
    #[case] caller_resource: TrackedResource,
    #[case] selector_name: &str,
    #[case] retdata: Vec<Felt>,
    #[case] failed: bool,
    #[case] expect_rejected: bool,
) {
    let dynamic_config =
        BatcherDynamicConfig { reject_invalid_signature_return_data: true, ..Default::default() };
    let tx = Transaction::Account(AccountTransaction::new_for_sequencing(executable_invoke_tx(
        invoke_tx_args!(),
    )));
    let caller = CallInfo {
        tracked_resource: caller_resource,
        inner_calls: vec![sig_call(selector_name, retdata, failed)],
        ..Default::default()
    };
    let tx_execution_info =
        TransactionExecutionInfo { execute_call_info: Some(caller), ..Default::default() };

    let result = create_transaction_filter(&dynamic_config).unwrap().check(&tx, &tx_execution_info);
    assert_eq!(result.is_err(), expect_rejected, "{result:?}");
}
