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

    // TODO: doc
    pub fn add_txs(
        &mut self,
        txs: &[Transaction],
    ) {
        log::info!("Worker executor: Adding {} transactions to worker executor.", txs.len());
        let (from_tx, to_tx) = self.worker_executor.add_txs(txs);
        log::info!(
            "Worker executor: Waiting for completion {from_tx}..{to_tx} now: {:?}",
            Instant::now()
        );
    }

    // TODO: doc
    pub fn get_processed_txs(
        &mut self,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let res = self.worker_executor.extract_execution_outputs(self.n_output_txs, None);
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
        log::info!("Worker executor: Adding {} transactions to worker executor.", txs.len());
        let (from_tx, to_tx) = self.worker_executor.add_txs(txs);
        log::info!(
            "Worker executor: Waiting for completion {from_tx}..{to_tx} now: {:?}",
            Instant::now()
        );
        // TODO(lior): Remove this check once tx streaming is supported.
        assert_eq!(
            from_tx, self.n_output_txs,
            "Can't add transaction after a partial result from an early run. Returned {} out of \
             {from_tx} transactions.",
            self.n_output_txs
        );
        self.worker_executor.scheduler.wait_for_completion(to_tx);
        log::info!("Worker executor: Waiting for completion done.");
        self.worker_pool.check_panic();
        let res = self.worker_executor.extract_execution_outputs(from_tx, Some(to_tx));
        log::info!("Worker executor: Extracted {} execution outputs.", res.len());

        self.n_output_txs += res.len();
        res
    }

    /// Finalizes the block creation and returns [BlockExecutionSummary].
    ///
    /// Every block must be closed with either `close_block` or `abort_block`.
    #[allow(clippy::result_large_err)]
    pub fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        log::info!("Worker executor: Closing block.");
        let worker_executor = &self.worker_executor;
        worker_executor.scheduler.halt();
        // TODO: Get n_committed_txs from the caller.
        let n_committed_txs = worker_executor.scheduler.get_n_committed_txs();
        let mut state_after_block =
            worker_executor.commit_chunk_and_recover_block_state(n_committed_txs);
        finalize_block(
            &worker_executor.bouncer,
            &mut state_after_block,
            &self.worker_executor.block_context,
        )
    }

    /// Returns true if the scheduler was halted. This happens when the block is full or the
    /// deadline is reached.
    pub fn is_done(&self) -> bool {
        self.worker_executor.scheduler.done()
    }

    /// Marks the block as aborted.
    pub fn abort_block(&mut self) {
        log::info!("Worker executor: Aborting block.");
        self.worker_executor.scheduler.halt();
    }
}
