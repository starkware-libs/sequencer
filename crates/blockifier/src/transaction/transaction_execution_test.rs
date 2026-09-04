use std::collections::HashSet;

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::fields::Calldata;
use starknet_api::{felt, invoke_tx_args, storage_key};
use starknet_types_core::felt::Felt;

use crate::context::{parse_blocked_storage_keys, BlockContext};
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use crate::transaction::test_utils::{
    create_test_init_data,
    default_all_resource_bounds,
    TestInitData,
};
use crate::transaction::transaction_execution::Transaction;
use crate::transaction::transactions::ExecutableTransaction;

const BLOCKED_STORAGE_KEY_ERROR_MESSAGE: &str = "Transaction accessed a blocked storage key.";

/// Executes an invoke tx that writes to (and then reads) `storage_key` in the test contract, with
/// `blocked_storage_keys` configured on the block context. With `nested`, the write happens in a
/// contract call made by the test contract to itself, one level deeper in the call tree.
fn execute_storage_write_tx(
    blocked_storage_keys: &str,
    storage_key: Felt,
    nested: bool,
) -> TransactionExecutionResult<TransactionExecutionInfo> {
    let block_context = BlockContext::create_for_account_testing().with_blocked_storage_keys(
        parse_blocked_storage_keys(blocked_storage_keys).unwrap(),
        BLOCKED_STORAGE_KEY_ERROR_MESSAGE.to_string(),
    );
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(
            &block_context.chain_info,
            CairoVersion::Cairo1(RunnableCairo1::Casm),
        );
    let storage_write_args = [storage_key, felt!(7_u8)];
    let calldata: Calldata = if nested {
        create_calldata(
            contract_address,
            "test_call_contract",
            &[
                vec![
                    *contract_address.0.key(),
                    selector_from_name("test_storage_read_write").0,
                    felt!(2_u8),
                ],
                storage_write_args.to_vec(),
            ]
            .concat(),
        )
    } else {
        create_calldata(contract_address, "test_storage_read_write", &storage_write_args)
    };
    let tx = executable_invoke_tx(invoke_tx_args! {
        sender_address: account_address,
        calldata,
        resource_bounds: default_all_resource_bounds(),
        nonce: nonce_manager.next(account_address),
    });
    Transaction::Account(AccountTransaction::new_for_sequencing(tx))
        .execute(&mut state, &block_context)
}

fn assert_blocked(result: TransactionExecutionResult<TransactionExecutionInfo>) {
    let error = result.expect_err("Access to a blocked storage key should fail the tx.");
    assert_matches!(
        &error,
        TransactionExecutionError::BlockedStorageKeyAccessed { message }
            if message == BLOCKED_STORAGE_KEY_ERROR_MESSAGE
    );
    assert_eq!(error.to_string(), BLOCKED_STORAGE_KEY_ERROR_MESSAGE);
}

fn assert_executed(result: TransactionExecutionResult<TransactionExecutionInfo>) {
    let tx_execution_info = result.expect("Tx should execute successfully.");
    assert!(!tx_execution_info.is_reverted(), "{:?}", tx_execution_info.revert_error);
}

#[rstest]
#[case::empty_blocklist("", felt!(0x10_u8))]
#[case::unrelated_key_blocked("0x10", felt!(0x11_u8))]
#[case::unrelated_long_key_blocked(
    "0x3f1abc55d5d1c9d3f6a8f0e2b7c4d9e1f2a3b4c5d6e7f8091a2b3c4d5e6f70",
    felt!("0x3f1abc55d5d1c9d3f6a8f0e2b7c4d9e1f2a3b4c5d6e7f8091a2b3c4d5e6f71")
)]
// A trailing comma must not turn into a blocked key 0x0.
#[case::trailing_comma_does_not_block_key_zero("0x10,", felt!(0x0_u8))]
fn test_blocked_storage_keys_do_not_affect_other_keys(
    #[case] blocked_storage_keys: &str,
    #[case] accessed_storage_key: Felt,
    #[values(false, true)] nested: bool,
) {
    assert_executed(execute_storage_write_tx(blocked_storage_keys, accessed_storage_key, nested));
}

#[rstest]
#[case::single_digit_key("0x1", felt!(0x1_u8))]
#[case::key_zero("0x0", felt!(0x0_u8))]
#[case::two_digit_key("0x10", felt!(0x10_u8))]
#[case::key_in_list("0x5,0x10,0x7", felt!(0x10_u8))]
#[case::hex_letter_key("0xab", felt!(0xab_u8))]
#[case::long_key(
    "0x3f1abc55d5d1c9d3f6a8f0e2b7c4d9e1f2a3b4c5d6e7f8091a2b3c4d5e6f70",
    felt!("0x3f1abc55d5d1c9d3f6a8f0e2b7c4d9e1f2a3b4c5d6e7f8091a2b3c4d5e6f70")
)]
// Normalization: surrounding whitespace, uppercase digits and leading zeros.
#[case::whitespace_around_key(" 0x10 , 0x20 ", felt!(0x10_u8))]
#[case::uppercase_digits("0xAB", felt!(0xab_u8))]
#[case::leading_zeros(
    "0x0000000000000000000000000000000000000000000000000000000000000010",
    felt!(0x10_u8)
)]
fn test_blocked_storage_key_access_fails_tx(
    #[case] blocked_storage_keys: &str,
    #[case] accessed_storage_key: Felt,
    #[values(false, true)] nested: bool,
) {
    assert_blocked(execute_storage_write_tx(blocked_storage_keys, accessed_storage_key, nested));
}

#[test]
fn test_parse_blocked_storage_keys() {
    assert_eq!(parse_blocked_storage_keys("").unwrap(), HashSet::new());
    assert_eq!(
        parse_blocked_storage_keys(" 0x1, ,0x10,").unwrap(),
        HashSet::from([storage_key!(0x1_u8), storage_key!(0x10_u8)])
    );
    // Not a hex number.
    assert!(parse_blocked_storage_keys("0x10,0xzz").is_err());
    // Out of the storage key range (2^251).
    assert!(
        parse_blocked_storage_keys(
            "0x800000000000000000000000000000000000000000000000000000000000000"
        )
        .is_err()
    );
}
