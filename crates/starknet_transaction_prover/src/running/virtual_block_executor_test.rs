use assert_matches::assert_matches;
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::bouncer::{BouncerConfig, BouncerWeights};
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::utils::get_chain_info;
use rstest::rstest;
use starknet_api::abi::abi_utils::{get_storage_var_address, selector_from_name};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};

use crate::errors::VirtualBlockExecutorError;
use crate::running::virtual_block_executor::{
    RpcVirtualBlockExecutor,
    RpcVirtualBlockExecutorConfig,
    VirtualBlockExecutor,
};
use crate::test_utils::{
    resolve_test_mode,
    resource_bounds_for_client_side_tx,
    rpc_virtual_block_executor,
    SENDER_ADDRESS,
    STRK_TOKEN_ADDRESS,
    TEST_BLOCK_NUMBER,
};

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
/// NODE_URL=https://your-rpc-node cargo test -p starknet_transaction_prover -- --ignored
/// ```
#[rstest]
#[ignore] // Requires RPC access 
fn test_execute_constructed_balance_of_transaction(
    rpc_virtual_block_executor: RpcVirtualBlockExecutor,
) {
    // Construct a balanceOf transaction (with execution flags set).
    let (tx, tx_hash) = construct_balance_of_invoke();

    // Create the virtual block executor.
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // Execute the transaction.
    let result = rpc_virtual_block_executor
        .execute(
            BlockId::Number(BlockNumber(TEST_BLOCK_NUMBER)),
            contract_class_manager,
            vec![(tx, tx_hash)],
        )
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
        result.base_block_info.block_context.block_info().block_number,
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

/// Constructs the test invoke transaction used by the simulate/prefetch integration tests.
fn construct_privacy_invoke() -> (InvokeTransaction, TransactionHash) {
    let tx = invoke_tx(invoke_tx_args! {
        sender_address: ContractAddress::try_from(
            felt!("0x037ee64c5681f8d1eea73429144d6a5c0ef271759a1d4342de13cef520fe35a7")
        ).unwrap(),
        calldata: calldata![
            felt!("0x70a5da4f557b77a9c54546e4bcc900806e28793d8e3eaaa207428d2387249b7"),
            felt!("0x35a73cd311a05d46deda634c5ee045db92f811b4e74bca4437fcb5302b7af33"),
            felt!("0x1"),
            felt!("0x037ee64c5681f8d1eea73429144d6a5c0ef271759a1d4342de13cef520fe35a7")
        ],
        resource_bounds: ValidResourceBounds::AllResources(resource_bounds_for_client_side_tx()),
        nonce: Nonce(felt!("0x21a")),
    });

    let tx_hash = Transaction::Invoke(tx.clone())
        .calculate_transaction_hash(&ChainId::IntegrationSepolia)
        .unwrap();
    (tx, tx_hash)
}

/// Integration test for executing a transaction with simulate-based state prefetch.
///
/// Runs `execute` with `prefetch_state: true`, which calls `starknet_simulateTransactions`
/// with `RETURN_INITIAL_READS` to prefetch state before execution, then executes the
/// transaction using the prefetched state.
///
/// # Running
///
/// ```bash
/// # Record:
/// RECORD_RPC_RECORDS=1 NODE_URL=http://<privacy-env-node>/rpc/v0_10 \
///     cargo test -p starknet_transaction_prover test_execute_with_prefetch -- --ignored
///
/// # Offline (after recording):
/// cargo test -p starknet_transaction_prover test_execute_with_prefetch -- --ignored
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC records or a live pathfinder v0.10 node
async fn test_execute_with_prefetch() {
    let test_mode = resolve_test_mode("test_execute_with_prefetch").await;
    let rpc_url = test_mode.rpc_url();

    let result = tokio::task::spawn_blocking(move || {
        let chain_info = get_chain_info(&ChainId::IntegrationSepolia, None);
        let block_id = BlockId::Latest;

        let mut executor = RpcVirtualBlockExecutor::new(
            rpc_url,
            chain_info,
            block_id,
            RpcVirtualBlockExecutorConfig { prefetch_state: true, ..Default::default() },
        );
        executor.validate_txs = false;

        let (tx, tx_hash) = construct_privacy_invoke();

        let contract_class_manager =
            ContractClassManager::start(ContractClassManagerConfig::default());

        executor.execute(block_id, contract_class_manager, vec![(tx, tx_hash)])
    })
    .await
    .unwrap();

    test_mode.finalize();

    let result = result.expect("execute with prefetch should succeed");

    assert_eq!(result.execution_outputs.len(), 1, "Should have exactly one execution output");

    let (execution_info, _) = &result.execution_outputs[0];
    assert!(
        !execution_info.is_reverted(),
        "Transaction should not revert. Error: {:?}",
        execution_info.revert_error
    );
}

/// Verifies that a transaction is rejected when the bouncer config has tight capacity limits.
///
/// Sets `n_txs: 0` so that any transaction exceeds the block capacity, and asserts that
/// execution returns `TransactionExecutionError` with a "Transaction size exceeds" message.
#[rstest]
#[ignore] // Requires RPC access
fn test_execute_rejected_by_tight_bouncer_limits(
    rpc_virtual_block_executor: RpcVirtualBlockExecutor,
) {
    // Override the bouncer config with zero capacity so any transaction is too large.
    let mut executor = rpc_virtual_block_executor;
    executor.config.bouncer_config = BouncerConfig {
        block_max_capacity: BouncerWeights { n_txs: 0, ..BouncerWeights::max() },
        ..Default::default()
    };

    let (tx, tx_hash) = construct_balance_of_invoke();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    let error = match executor.execute(
        BlockId::Number(BlockNumber(TEST_BLOCK_NUMBER)),
        contract_class_manager,
        vec![(tx, tx_hash)],
    ) {
        Err(error) => error,
        Ok(_) => panic!("Execution should fail when bouncer capacity is zero"),
    };

    assert_matches!(
        error,
        VirtualBlockExecutorError::TransactionExecutionError(msg)
            if msg.contains("Transaction size exceeds the maximum block capacity")
    );
}
