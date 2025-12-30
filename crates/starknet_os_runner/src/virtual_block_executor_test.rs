use std::sync::Arc;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use rstest::rstest;
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};

use crate::test_utils::{
    rpc_virtual_block_executor,
    SENDER_ADDRESS,
    STRK_TOKEN_ADDRESS,
    TEST_BLOCK_NUMBER,
};
use crate::virtual_block_executor::{RpcVirtualBlockExecutor, VirtualBlockExecutor};

/// Constructs an Invoke transaction that calls `balanceOf` on the STRK token contract.
///
/// Since we skip validation and fee charging, we can use dummy values for signature,
/// nonce, and resource bounds.
fn construct_balance_of_invoke() -> (InvokeTransaction, TransactionHash) {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let sender = ContractAddress::try_from(SENDER_ADDRESS).unwrap();

    // Calldata for account's __execute__ (Cairo 0 OZ account format):
    // [call_array_len, call_array..., calldata_len, calldata...]
    // where call_array is [(to, selector, data_offset, data_len), ...]
    let balance_of_selector = selector_from_name("balanceOf");
    let calldata = calldata![
        felt!("1"),            // call_array_len - number of calls
        *strk_token.0.key(),   // call_array[0].to - contract to call
        balance_of_selector.0, // call_array[0].selector - function selector
        felt!("0"),            // call_array[0].data_offset - offset into calldata
        felt!("1"),            // call_array[0].data_len - length of this call's data
        felt!("1"),            // calldata_len - total calldata length
        *sender.0.key()        // calldata[0] - address to check balance of
    ];

    // Use a high nonce to satisfy the non-strict nonce check (nonce >= account_nonce).
    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: sender,
        calldata,
        nonce: Nonce(felt!("0x1000000")),
    });

    let tx_hash = Transaction::Invoke(invoke_tx.clone())
        .calculate_transaction_hash(&ChainId::Mainnet)
        .unwrap();
    (invoke_tx, tx_hash)
}

/// Integration test for RpcVirtualBlockExecutor with a constructed transaction.
///
/// This test:
/// 1. Constructs a balanceOf call on the STRK token contract
/// 2. Executes it using RpcVirtualBlockExecutor (without validation/fees)
/// 3. Verifies that execution succeeds and initial_reads contains storage
///
/// # Environment Variables
///
/// - `NODE_URL`: Required. URL of a Starknet mainnet RPC node.
///
/// # Running
///
/// ```bash
/// NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner -- --ignored
/// ```
#[rstest]
#[ignore] // Requires RPC access
#[tokio::test]
async fn test_execute_constructed_balance_of_transaction(
    rpc_virtual_block_executor: Arc<RpcVirtualBlockExecutor>,
) {
    // Construct a balanceOf transaction (with execution flags set).
    let (tx, tx_hash) = construct_balance_of_invoke();

    // Create the virtual block executor.
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // Execute the transaction.
    let result = rpc_virtual_block_executor
        .execute(BlockNumber(TEST_BLOCK_NUMBER), contract_class_manager, vec![(tx, tx_hash)])
        .await
        .unwrap();

    // Verify execution produced output.
    assert_eq!(result.execution_outputs.len(), 1, "Should have exactly one execution output");

    let (execution_info, _) = &result.execution_outputs[0];

    // Verify execution succeeded (no revert).
    assert!(
        !execution_info.is_reverted(),
        "Transaction should not revert. Error: {:?}",
        execution_info.revert_error
    );

    // Verify state was accessed.
    assert!(
        !result.initial_reads.nonces.is_empty(),
        "initial_reads.nonces should be non-empty (sender nonce was read)"
    );
    assert!(
        !result.initial_reads.class_hashes.is_empty(),
        "initial_reads.class_hashes should be non-empty (account class was read)"
    );
    assert!(
        !result.initial_reads.storage.is_empty(),
        "initial_reads.storage should be non-empty (balance storage was read)"
    );

    // Verify executed class hashes were captured.
    assert!(!result.executed_class_hashes.is_empty(), "executed_class_hashes should be non-empty");

    // Verify the specific ERC20 balance storage key was read.
    // ERC20 contracts store balances in "ERC20_balances" mapping keyed by address.
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let sender = ContractAddress::try_from(SENDER_ADDRESS).unwrap();
    let balance_storage_key = get_storage_var_address("ERC20_balances", &[*sender.0.key()]);
    assert!(
        result.initial_reads.storage.contains_key(&(strk_token, balance_storage_key)),
        "initial_reads.storage should contain the ERC20_balances storage key for the sender"
    );

    // Verify block context was captured.
    assert_eq!(
        result.block_context.block_info().block_number,
        BlockNumber(TEST_BLOCK_NUMBER),
        "Block context should have the correct block number"
    );

    println!(
        "Execution succeeded: {} nonces, {} class hashes, {} storage keys read",
        result.initial_reads.nonces.len(),
        result.initial_reads.class_hashes.len(),
        result.initial_reads.storage.len()
    );
}
