use std::sync::Arc;
use std::time::{Duration, Instant};

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use rstest::rstest;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::nonce;

use crate::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use crate::blockifier::config::WorkerPoolConfig;
use crate::blockifier::transaction_executor::TransactionExecutorError;
use crate::concurrency::worker_pool::WorkerPool;
use crate::context::BlockContext;
use crate::state::cached_state::CachedState;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::maybe_dummy_block_hash_and_number;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::test_utils::{create_test_init_data, emit_n_events_tx, TestInitData};
use crate::transaction::transaction_execution::Transaction;

fn get_txs<const N: usize>(txs: [AccountTransaction; N]) -> Vec<Transaction> {
    txs.into_iter().map(Transaction::Account).collect()
}

struct TestData {
    pool: Arc<WorkerPool<CachedState<CachedState<DictStateReader>>>>,
    tx_executor: ConcurrentTransactionExecutor<CachedState<DictStateReader>>,
    account_address: ContractAddress,
    contract_address: ContractAddress,
    max_n_events_in_block: usize,
}

fn get_test_data(block_deadline: Option<Instant>) -> TestData {
    let pool = Arc::new(WorkerPool::start(&WorkerPoolConfig::create_for_testing()));

    let max_n_events_in_block = 10;
    let block_context = BlockContext::create_for_bouncer_testing(max_n_events_in_block);

    let TestInitData { state, account_address, contract_address, .. } = create_test_init_data(
        &block_context.chain_info,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
    );

    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);

    let tx_executor = ConcurrentTransactionExecutor::start_block(
        state,
        block_context,
        block_number_hash_pair,
        pool.clone(),
        block_deadline,
    )
    .unwrap();

    TestData { pool, tx_executor, account_address, contract_address, max_n_events_in_block }
}

fn test_txs(
    account_address: ContractAddress,
    contract_address: ContractAddress,
    max_n_events_in_block: usize,
) -> (Vec<Transaction>, Vec<Transaction>) {
    let txs0 = get_txs([
        emit_n_events_tx(1, account_address, contract_address, nonce!(0_u32)),
        // Transaction too big.
        emit_n_events_tx(
            max_n_events_in_block + 1,
            account_address,
            contract_address,
            nonce!(1_u32),
        ),
        emit_n_events_tx(3, account_address, contract_address, nonce!(1_u32)),
    ]);

    let txs1 = get_txs([
        emit_n_events_tx(1, account_address, contract_address, nonce!(2_u32)),
        // No room for this in block - execution should halt.
        emit_n_events_tx(7, account_address, contract_address, nonce!(3_u32)),
        // Should not be processed since the execution halted.
        emit_n_events_tx(1, account_address, contract_address, nonce!(3_u32)),
    ]);

    (txs0, txs1)
}

#[rstest]
#[case::zero_txs(0, None)]
#[case::one_tx(1, Some(nonce!(1)))]
#[case::two_txs(2, Some(nonce!(1)))]
#[case::three_txs(3, Some(nonce!(2)))]
#[case::four_txs(4, Some(nonce!(3)))]
fn test_concurrent_transaction_executor(
    #[case] final_n_executed_txs: usize,
    #[case] expected_nonce: Option<Nonce>,
) {
    let TestData {
        pool,
        mut tx_executor,
        account_address,
        contract_address,
        max_n_events_in_block,
    } = get_test_data(None);

    let (txs0, txs1) = test_txs(account_address, contract_address, max_n_events_in_block);

    // Run.
    let results0 = tx_executor.add_txs_and_wait(&txs0);
    let results1 = tx_executor.add_txs_and_wait(&txs1);

    // Check execution results.
    assert_eq!(results0.len(), 3, "The transaction results are {results0:?}");

    assert!(results0[0].is_ok(), "Transaction Failed: {:?}", results0[0]);
    assert_matches!(
        results0[1].as_ref().unwrap_err(),
        TransactionExecutorError::TransactionExecutionError(
            TransactionExecutionError::TransactionTooLarge { .. }
        )
    );
    assert!(results0[2].is_ok(), "Transaction Failed: {:?}", results0[2]);

    assert_eq!(results1.len(), 1, "The transaction results are {results1:?}");
    assert!(results1[0].is_ok(), "Transaction Failed: {:?}", results1[0]);

    // Close the block.
    let block_summary = tx_executor.close_block(final_n_executed_txs).unwrap();
    assert_eq!(
        block_summary.state_diff.address_to_nonce.get(&account_address).cloned(),
        expected_nonce
    );

    // End test by calling pool.join().
    drop(tx_executor);
    Arc::try_unwrap(pool).expect("More than one instance of worker pool exists").join();
}

#[rstest]
fn test_concurrent_transaction_executor_stream_txs() {
    let TestData {
        pool,
        mut tx_executor,
        account_address,
        contract_address,
        max_n_events_in_block,
    } = get_test_data(None);

    let (txs0, txs1) = test_txs(account_address, contract_address, max_n_events_in_block);

    // Run.
    tx_executor.add_txs(&txs0);
    tx_executor.add_txs(&txs1);

    // Collect the results.
    let mut results = vec![];
    let start_time = Instant::now();
    while !tx_executor.is_done() {
        if start_time.elapsed() > Duration::from_secs(10) {
            panic!("Test timed out: tx_executor did not finish within 10 seconds");
        }
        results.extend(tx_executor.get_new_results());
        std::thread::sleep(Duration::from_millis(1));
    }
    results.extend(tx_executor.get_new_results());

    // Check execution results.
    assert_eq!(results.len(), 4, "The transaction results are {results:?}");

    assert!(results[0].is_ok(), "Transaction Failed: {:?}", results[0]);
    assert_matches!(
        results[1].as_ref().unwrap_err(),
        TransactionExecutorError::TransactionExecutionError(
            TransactionExecutionError::TransactionTooLarge { .. }
        )
    );
    assert!(results[2].is_ok(), "Transaction Failed: {:?}", results[2]);
    assert!(results[3].is_ok(), "Transaction Failed: {:?}", results[3]);

    // Close the block.
    let block_summary = tx_executor.close_block(results.len()).unwrap();
    assert_eq!(block_summary.state_diff.address_to_nonce[&account_address], nonce!(3_u32));

    // End test by calling pool.join().
    drop(tx_executor);
    Arc::try_unwrap(pool).expect("More than one instance of worker pool exists").join();
}

#[rstest]
fn test_concurrent_transaction_executor_abort() {
    let TestData { pool, mut tx_executor, .. } = get_test_data(None);

    // Not calling `abort_block` would cause the `join` below to hang.
    tx_executor.abort_block();

    drop(tx_executor);
    Arc::try_unwrap(pool).expect("More than one instance of worker pool exists").join();
}

#[rstest]
fn test_concurrent_transaction_executor_deadline() {
    let deadline = Instant::now();
    let TestData { pool, mut tx_executor, account_address, contract_address, .. } =
        get_test_data(Some(deadline));

    let txs0 = get_txs([emit_n_events_tx(1, account_address, contract_address, nonce!(0_u32))]);

    let results0 = tx_executor.add_txs_and_wait(&txs0);
    // Expect no results since the deadline passed.
    assert_eq!(results0.len(), 0);
    assert_eq!(tx_executor.worker_executor.scheduler.get_n_committed_txs(), 0);

    let block_summary = tx_executor.close_block(0).unwrap();
    assert!(block_summary.state_diff.address_to_nonce.get(&account_address).is_none());

    drop(tx_executor);
    Arc::try_unwrap(pool).expect("More than one instance of worker pool exists").join();
}
