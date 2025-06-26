use std::sync::{Arc, Mutex};
use std::time::Instant;

use starknet_api::block::BlockHashAndNumber;

use crate::blockifier::block::pre_process_block;
use crate::blockifier::transaction_executor::{
    finalize_block,
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutorResult,
};
use crate::bouncer::Bouncer;
use crate::concurrency::worker_logic::WorkerExecutor;
use crate::concurrency::worker_pool::WorkerPool;
use crate::context::BlockContext;
use crate::state::cached_state::CachedState;
use crate::state::state_api::{StateReader, StateResult};
use crate::transaction::transaction_execution::Transaction;

#[cfg(test)]
#[path = "concurrent_transaction_executor_test.rs"]
pub mod concurrent_transaction_executor_test;

pub struct ConcurrentTransactionExecutor<S: StateReader> {
    worker_executor: Arc<WorkerExecutor<CachedState<S>>>,
    worker_pool: Arc<WorkerPool<CachedState<S>>>,
    /// The number of transactions that have been outputted by the executor.
    /// See [Self::get_new_results].
    n_output_txs: usize,
}

impl<S: StateReader + Send + 'static> ConcurrentTransactionExecutor<S> {
    /// Creates a new [ConcurrentTransactionExecutor] for a new block.
    /// The executor is added to the [WorkerPool] and will be executed in separate threads.
    pub fn start_block(
        initial_state_reader: S,
        block_context: BlockContext,
        old_block_number_and_hash: Option<BlockHashAndNumber>,
        worker_pool: Arc<WorkerPool<CachedState<S>>>,
        block_deadline: Option<Instant>,
    ) -> StateResult<Self> {
        let mut block_state = CachedState::new(initial_state_reader);
        pre_process_block(
            &mut block_state,
            old_block_number_and_hash,
            block_context.block_info().block_number,
            &block_context.versioned_constants.os_constants,
        )?;

        let bouncer_config = block_context.bouncer_config.clone();
        let worker_executor = Arc::new(WorkerExecutor::initialize(
            block_state,
            vec![],
            block_context.into(),
            Mutex::new(Bouncer::new(bouncer_config)).into(),
            block_deadline,
        ));
        worker_pool.run(worker_executor.clone());

        Ok(Self { worker_executor, worker_pool: worker_pool.clone(), n_output_txs: 0 })
    }

<<<<<<< HEAD
    /// Similar to [ConcurrentTransactionExecutor::start_block], except that [pre_process_block] is
    /// not called. Used for testing purposes.
||||||| 2452f56bc
    /// Similar to [start_block], except that [pre_process_block] is not called.
    /// Used for testing purposes.
=======
    /// Similar to [Self::start_block], except that [pre_process_block] is not called.
    /// Used for testing purposes.
>>>>>>> origin/main-v0.14.0
    #[cfg(any(feature = "testing", test))]
    pub fn new_for_testing(
        block_state: CachedState<S>,
        block_context: BlockContext,
        worker_pool: Arc<WorkerPool<CachedState<S>>>,
        block_deadline: Option<Instant>,
    ) -> Self {
        let bouncer_config = block_context.bouncer_config.clone();
        let worker_executor = Arc::new(WorkerExecutor::initialize(
            block_state,
            vec![],
            block_context.into(),
            Mutex::new(Bouncer::new(bouncer_config)).into(),
            block_deadline,
        ));
        worker_pool.run(worker_executor.clone());

        Self { worker_executor, worker_pool: worker_pool.clone(), n_output_txs: 0 }
    }

    /// Starts executing the given transactions.
    pub fn add_txs(&mut self, txs: &[Transaction]) {
        self.worker_executor.add_txs(txs);
    }

    /// Returns the new execution outputs of the transactions that were processed so far, starting
    /// from the last call to `get_new_results`.
    pub fn get_new_results(
        &mut self,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let res = self.worker_executor.extract_execution_outputs(self.n_output_txs);
        self.worker_pool.check_panic();
        self.n_output_txs += res.len();
        res
    }

    /// Adds the given transactions to the block and waits for them to be executed.
    ///
    /// Returns the execution results. Note that the execution results may be incomplete
    /// if the block is halted.
    pub fn add_txs_and_wait(
        &mut self,
        txs: &[Transaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let (from_tx, to_tx) = self.worker_executor.add_txs(txs);
        assert_eq!(
            from_tx, self.n_output_txs,
            "Can't add transaction after a partial result from an early run. Returned {} out of \
             {from_tx} transactions.",
            self.n_output_txs
        );
        self.worker_executor.scheduler.wait_for_completion(to_tx);
        self.get_new_results()
    }

    /// Finalizes the block creation and returns [BlockExecutionSummary].
    ///
    /// Every block must be closed with either `close_block` or `abort_block`.
    pub fn close_block(
        &mut self,
        final_n_executed_txs: usize,
    ) -> TransactionExecutorResult<BlockExecutionSummary> {
        log::info!("Worker executor: Closing block.");
        let worker_executor = &self.worker_executor;
        worker_executor.scheduler.halt();

        let n_committed_txs = worker_executor.scheduler.get_n_committed_txs();
        assert!(
            final_n_executed_txs <= n_committed_txs,
            "Close block requested with {final_n_executed_txs} transactions, but only \
             {n_committed_txs} transactions were committed."
        );

        let mut state_after_block =
            worker_executor.commit_chunk_and_recover_block_state(final_n_executed_txs);
        finalize_block(
            &worker_executor.bouncer,
            &mut state_after_block,
            &self.worker_executor.block_context,
        )
    }

    /// Returns `true` if the scheduler was halted. This happens when the block is full or the
    /// deadline is reached.
    pub fn is_done(&self) -> bool {
        self.worker_executor.scheduler.done()
    }

    /// Halts the scheduler, to allow the worker threads to continue to the next block.
    pub fn abort_block(&mut self) {
        log::info!("Worker executor: Aborting block.");
        self.worker_executor.scheduler.halt();
    }
}
