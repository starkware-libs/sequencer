use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use rstest::rstest;
use starknet_api::nonce;

use crate::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use crate::blockifier::config::TransactionExecutorConfig;
use crate::blockifier::transaction_executor::TransactionExecutorError;
use crate::concurrency::worker_pool::WorkerPool;
use crate::context::BlockContext;
use crate::test_utils::maybe_dummy_block_hash_and_number;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::test_utils::{create_test_init_data, emit_n_events_tx, TestInitData};
use crate::transaction::transaction_execution::Transaction;

#[rstest]
fn test_concurrent_transaction_executor() {
    let config = TransactionExecutorConfig::create_for_testing(true);
    let max_n_events_in_block = 10;
    let block_context = BlockContext::create_for_bouncer_testing(max_n_events_in_block);

    let TestInitData { state, account_address, contract_address, .. } = create_test_init_data(
        &block_context.chain_info,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
    );

    let pool = Arc::new(WorkerPool::start(config.stack_size, config.concurrency_config.clone()));

    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);

    let mut tx_executor = ConcurrentTransactionExecutor::start_block(
        state,
        block_context,
        block_number_hash_pair,
        pool.clone(),
    )
    .unwrap();

    let txs: Vec<Transaction> = [
        emit_n_events_tx(1, account_address, contract_address, nonce!(0_u32)),
        // Transaction too big.
        emit_n_events_tx(
            max_n_events_in_block + 1,
            account_address,
            contract_address,
            nonce!(1_u32),
        ),
        emit_n_events_tx(8, account_address, contract_address, nonce!(1_u32)),
        // No room for this in block - execution should halt.
        emit_n_events_tx(2, account_address, contract_address, nonce!(2_u32)),
        // Has room for this one, but should not be processed at all.
        emit_n_events_tx(1, account_address, contract_address, nonce!(3_u32)),
    ]
    .into_iter()
    .map(Transaction::Account)
    .collect();

    // Run.
    let results = tx_executor.add_transactions_and_wait(&txs);

    // Check execution results.
    let expected_offset = 3;
    assert_eq!(results.len(), expected_offset);

    assert!(results[0].is_ok());
    assert_matches!(
        results[1].as_ref().unwrap_err(),
        TransactionExecutorError::TransactionExecutionError(
            TransactionExecutionError::TransactionTooLarge { .. }
        )
    );
    assert!(results[2].is_ok());

    // Check state.
    // assert_eq!(
    //     tx_executor
    //         .block_state
    //         .as_ref()
    //         .expect(BLOCK_STATE_ACCESS_ERR)
    //         .get_nonce_at(account_address)
    //         .unwrap(),
    //     nonce!(2_u32)
    // );

    // Check idempotency: excess transactions should not be added.
    // let remaining_txs = &txs[expected_offset..];
    // let remaining_tx_results = tx_executor.add_transactions_and_wait(remaining_txs, None);
    // assert_eq!(remaining_tx_results.len(), 0);

    // // Reset the bouncer and add the remaining transactions.
    // tx_executor.bouncer =
    //     Mutex::new(Bouncer::new(tx_executor.block_context.bouncer_config.clone())).into();
    // let remaining_tx_results = tx_executor.add_transactions_and_wait(remaining_txs, None);

    // assert_eq!(remaining_tx_results.len(), 2);
    // assert!(remaining_tx_results[0].is_ok());
    // assert!(remaining_tx_results[1].is_ok());
    // assert_eq!(
    //     tx_executor
    //         .block_state
    //         .as_ref()
    //         .expect(BLOCK_STATE_ACCESS_ERR)
    //         .get_nonce_at(account_address)
    //         .unwrap(),
    //     nonce!(4_u32)
    // );

    // End test by calling pool.join().
    drop(tx_executor);
    Arc::try_unwrap(pool).expect("More than one instance of worker pool exists").join();
}
