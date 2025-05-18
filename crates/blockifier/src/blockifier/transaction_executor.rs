use std::mem;
use std::panic::{self, catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use apollo_infra_utils::tracing::LogCompatibleToStringExt;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use starknet_api::block::BlockHashAndNumber;
use thiserror::Error;

use crate::blockifier::block::pre_process_block;
use crate::blockifier::config::TransactionExecutorConfig;
use crate::bouncer::{Bouncer, BouncerWeights, CasmHashComputationData};
use crate::concurrency::utils::AbortIfPanic;
use crate::concurrency::worker_logic::WorkerExecutor;
use crate::context::BlockContext;
use crate::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps, TransactionalState};
use crate::state::errors::StateError;
use crate::state::state_api::{StateReader, StateResult};
use crate::state::stateful_compression::{allocate_aliases_in_storage, compress, CompressionError};
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::TransactionExecutionInfo;
use crate::transaction::transaction_execution::Transaction;
use crate::transaction::transactions::ExecutableTransaction;

#[cfg(test)]
#[path = "transaction_executor_test.rs"]
pub mod transaction_executor_test;

pub const BLOCK_STATE_ACCESS_ERR: &str = "Error: The block state should be `Some`.";
pub const DEFAULT_STACK_SIZE: usize = 60 * 1024 * 1024;

pub type TransactionExecutionOutput = (TransactionExecutionInfo, StateMaps);

#[derive(Debug, Error)]
pub enum TransactionExecutorError {
    #[error("Transaction cannot be added to the current block, block capacity reached.")]
    BlockFull,
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error(transparent)]
    CompressionError(#[from] CompressionError),
}

impl LogCompatibleToStringExt for TransactionExecutorError {}

pub type TransactionExecutorResult<T> = Result<T, TransactionExecutorError>;

pub struct BlockExecutionSummary {
    pub state_diff: CommitmentStateDiff,
    pub compressed_state_diff: Option<CommitmentStateDiff>,
    pub bouncer_weights: BouncerWeights,
    pub casm_hash_computation_data: CasmHashComputationData,
}

/// A transaction executor, used for building a single block.
pub struct TransactionExecutor<S: StateReader> {
    pub block_context: BlockContext,
    pub bouncer: Bouncer,
    // Note: this config must not affect the execution result (e.g. state diff and traces).
    pub config: TransactionExecutorConfig,

    // State-related fields.
    // The transaction executor operates at the block level. In concurrency mode, it moves the
    // block state to the worker executor - operating at the chunk level - and gets it back after
    // committing the chunk. The block state is wrapped with an Option<_> to allow setting it to
    // `None` while it is moved to the worker executor.
    pub block_state: Option<CachedState<S>>,
}

impl<S: StateReader> TransactionExecutor<S> {
    /// Performs pre-processing required for block building before creating the executor.
    pub fn pre_process_and_create(
        initial_state_reader: S,
        block_context: BlockContext,
        old_block_number_and_hash: Option<BlockHashAndNumber>,
        config: TransactionExecutorConfig,
    ) -> StateResult<Self> {
        let mut block_state = CachedState::new(initial_state_reader);
        pre_process_block(
            &mut block_state,
            old_block_number_and_hash,
            block_context.block_info().block_number,
            &block_context.versioned_constants.os_constants,
        )?;
        Ok(Self::new(block_state, block_context, config))
    }

    // TODO(Yoni): consider making this c-tor private.
    pub fn new(
        block_state: CachedState<S>,
        block_context: BlockContext,
        config: TransactionExecutorConfig,
    ) -> Self {
        let bouncer_config = block_context.bouncer_config.clone();
        // Note: the state might not be empty even at this point; it is the creator's
        // responsibility to tune the bouncer according to pre and post block process.
        Self {
            block_context,
            bouncer: Bouncer::new(bouncer_config),
            config,
            block_state: Some(block_state),
        }
    }

    /// Executes the given transaction on the state maintained by the executor.
    /// Returns the execution result (info or error) if there is room for the transaction;
    /// Otherwise, returns BlockFull error.
    pub fn execute(
        &mut self,
        tx: &Transaction,
    ) -> TransactionExecutorResult<TransactionExecutionOutput> {
        let mut transactional_state = TransactionalState::create_transactional(
            self.block_state.as_mut().expect(BLOCK_STATE_ACCESS_ERR),
        );

        // Executing a single transaction cannot be done in a concurrent mode.
        let concurrency_mode = false;
        let tx_execution_result =
            tx.execute_raw(&mut transactional_state, &self.block_context, concurrency_mode);
        match tx_execution_result {
            Ok(tx_execution_info) => {
                let state_diff = transactional_state.to_state_diff()?.state_maps;
                let tx_state_changes_keys = state_diff.keys();
                self.bouncer.try_update(
                    &transactional_state,
                    &tx_state_changes_keys,
                    &tx_execution_info.summarize(&self.block_context.versioned_constants),
                    &tx_execution_info.receipt.resources,
                    &self.block_context.versioned_constants,
                )?;
                transactional_state.commit();

                Ok((tx_execution_info, state_diff))
            }
            Err(error) => {
                transactional_state.abort();
                Err(TransactionExecutorError::TransactionExecutionError(error))
            }
        }
    }

    fn execute_txs_sequentially_inner(
        &mut self,
        txs: &[Transaction],
        execution_deadline: Option<Instant>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let mut results = Vec::new();
        for tx in txs {
            if let Some(deadline) = execution_deadline {
                if Instant::now() > deadline {
                    log::debug!("Execution timed out.");
                    break;
                }
            }
            match self.execute(tx) {
                Ok((tx_execution_info, state_diff)) => {
                    results.push(Ok((tx_execution_info, state_diff)))
                }
                Err(TransactionExecutorError::BlockFull) => break,
                Err(error) => results.push(Err(error)),
            }
        }
        results
    }

    /// Returns the state diff and the block weights.
    // TODO(Aner): Consume "self", i.e., remove the reference, after removing the native blockifier.
    pub fn finalize(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        self.internal_finalize()
    }

    #[cfg(feature = "reexecution")]
    pub fn non_consuming_finalize(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        self.internal_finalize()
    }

    fn internal_finalize(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        log::debug!("Final block weights: {:?}.", self.bouncer.get_accumulated_weights());
        let block_state = self.block_state.as_mut().expect(BLOCK_STATE_ACCESS_ERR);
        let alias_contract_address = self
            .block_context
            .versioned_constants
            .os_constants
            .os_contract_addresses
            .alias_contract_address();
        if self.block_context.versioned_constants.enable_stateful_compression {
            allocate_aliases_in_storage(block_state, alias_contract_address)?;
        }
        let state_diff = block_state.to_state_diff()?.state_maps;
        let compressed_state_diff =
            if self.block_context.versioned_constants.enable_stateful_compression {
                Some(compress(&state_diff, block_state, alias_contract_address)?.into())
            } else {
                None
            };
        Ok(BlockExecutionSummary {
            state_diff: state_diff.into(),
            compressed_state_diff,
            bouncer_weights: *self.bouncer.get_accumulated_weights(),
            casm_hash_computation_data: mem::take(&mut self.bouncer.casm_hash_computation_data),
        })
    }
}

impl<S: StateReader + Send + Sync> TransactionExecutor<S> {
    /// Executes the given transactions on the state maintained by the executor.
    ///
    /// # Arguments:
    /// * `txs` - A slice of transactions to be executed
    /// * `timeout` - Optional duration specifying maximum execution time
    ///
    /// Returns a vector of `TransactionExecutorResult<TransactionExecutionOutput>`, containing the
    /// execution results for each transaction. The execution may stop early if the block becomes
    /// full.
    pub fn execute_txs(
        &mut self,
        txs: &[Transaction],
        timeout: Option<Duration>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let execution_deadline = timeout.map(|timeout| Instant::now() + timeout);
        if !self.config.concurrency_config.enabled {
            log::debug!("Executing transactions sequentially.");
            self.execute_txs_sequentially(txs, execution_deadline)
        } else {
            log::debug!("Executing transactions concurrently.");
            let chunk_size = self.config.concurrency_config.chunk_size;
            let n_workers = self.config.concurrency_config.n_workers;
            assert!(
                chunk_size > 0,
                "When running transactions concurrently the chunk size must be greater than 0. It \
                 equals {:?} ",
                chunk_size
            );
            assert!(
                n_workers > 0,
                "When running transactions concurrently the number of workers must be greater \
                 than 0. It equals {:?} ",
                n_workers
            );
            txs.chunks(chunk_size)
                .fold_while(Vec::new(), |mut results, chunk| {
                    let chunk_results = self.execute_chunk(chunk, execution_deadline);
                    if chunk_results.len() < chunk.len() {
                        // Block is full.
                        results.extend(chunk_results);
                        Done(results)
                    } else {
                        results.extend(chunk_results);
                        Continue(results)
                    }
                })
                .into_inner()
        }
    }

    fn execute_txs_sequentially(
        &mut self,
        txs: &[Transaction],
        execution_deadline: Option<Instant>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        #[cfg(not(feature = "cairo_native"))]
        return self.execute_txs_sequentially_inner(txs, execution_deadline);
        #[cfg(feature = "cairo_native")]
        {
            // TODO(meshi): find a way to access the contract class manager config from transaction
            // executor.
            let txs = txs.to_vec();
            std::thread::scope(|s| {
                std::thread::Builder::new()
                    // when running Cairo natively, the real stack is used and could get overflowed
                    // (unlike the VM where the stack is simulated in the heap as a memory segment).
                    //
                    // We pre-allocate the stack here, and not during Native execution (not trivial), so it
                    // needs to be big enough ahead.
                    // However, making it very big is wasteful (especially with multi-threading).
                    // So, the stack size should support calls with a reasonable gas limit, for extremely deep
                    // recursions to reach out-of-gas before hitting the bottom of the recursion.
                    //
                    // The gas upper bound is MAX_POSSIBLE_SIERRA_GAS, and sequencers must not raise it without
                    // adjusting the stack size.
                    .stack_size(self.config.stack_size)
                    .spawn_scoped(s, || self.execute_txs_sequentially_inner(&txs, execution_deadline))
                    .expect("Failed to spawn thread")
                    .join()
                    .expect("Failed to join thread.")
            })
        }
    }

    fn execute_chunk(
        &mut self,
        chunk: &[Transaction],
        execution_deadline: Option<Instant>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let block_state = self.block_state.take().expect("The block state should be `Some`.");
        let chunk_size = chunk.len();

        let worker_executor = Arc::new(WorkerExecutor::initialize(
            block_state,
            chunk,
            &self.block_context,
            Mutex::new(&mut self.bouncer),
            execution_deadline,
        ));

        // No thread pool implementation is needed here since we already have our scheduler. The
        // initialized threads below will "busy wait" for new tasks using the `run` method until the
        // chunk execution is completed, and then they will be joined together in a for loop.
        // TODO(barak, 01/07/2024): Consider using tokio and spawn tasks that will be served by some
        // upper level tokio thread pool (Runtime in tokio terminology).
        std::thread::scope(|s| {
            for _ in 0..self.config.concurrency_config.n_workers {
                let worker_executor = Arc::clone(&worker_executor);
                let _handle = std::thread::Builder::new()
                    // when running Cairo natively, the real stack is used and could get overflowed
                    // (unlike the VM where the stack is simulated in the heap as a memory segment).
                    //
                    // We pre-allocate the stack here, and not during Native execution (not trivial), so it
                    // needs to be big enough ahead.
                    // However, making it very big is wasteful (especially with multi-threading).
                    // So, the stack size should support calls with a reasonable gas limit, for extremely deep
                    // recursions to reach out-of-gas before hitting the bottom of the recursion.
                    //
                    // The gas upper bound is MAX_POSSIBLE_SIERRA_GAS, and sequencers must not raise it without
                    // adjusting the stack size.
                    .stack_size(self.config.stack_size)
                    .spawn_scoped(s, move || {
                        // Making sure that the program will abort if a panic accured while halting
                        // the scheduler.
                        let abort_guard = AbortIfPanic;
                        // If a panic is not handled or the handling logic itself panics, then we
                        // abort the program.
                        if let Err(err) = catch_unwind(AssertUnwindSafe(|| {
                            worker_executor.run();
                        })) {
                            // If the program panics here, the abort guard will exit the program.
                            // In this case, no panic message will be logged. Add the cargo flag
                            // --nocapture to log the panic message.

                            worker_executor.scheduler.halt();
                            abort_guard.release();
                            panic::resume_unwind(err);
                        }

                        abort_guard.release();
                    })
                    .expect("Failed to spawn thread.");
            }
        });

        let n_committed_txs = worker_executor.scheduler.get_n_committed_txs();
        let (abort_counter, abort_in_commit_counter, execute_counter, validate_counter) =
            worker_executor.metrics.get_metrics();
        log::debug!(
            "Concurrent execution done. Initial chunk size: {chunk_size}; Committed chunk size: \
             {n_committed_txs}; Execute counter: {execute_counter}; Validate counter: \
             {validate_counter}; Abort counter: {abort_counter}; Abort in commit counter: \
             {abort_in_commit_counter}"
        );
        let mut tx_execution_results = Vec::new();
        for execution_output in worker_executor.execution_outputs.iter() {
            if tx_execution_results.len() >= n_committed_txs {
                break;
            }
            let locked_execution_output = execution_output
                .lock()
                .expect("Failed to lock execution output.")
                .take()
                .expect("Output must be ready.");
            let tx_execution_output = locked_execution_output
                .result
                .map(|tx_execution_info| (tx_execution_info, locked_execution_output.state_diff))
                .map_err(TransactionExecutorError::from);
            tx_execution_results.push(tx_execution_output);
        }

        let block_state_after_commit = Arc::try_unwrap(worker_executor)
            .unwrap_or_else(|_| {
                panic!(
                    "To consume the block state, you must have only one strong reference to the \
                     worker executor factory. Consider dropping objects that hold a reference to \
                     it."
                )
            })
            .commit_chunk_and_recover_block_state(n_committed_txs);
        self.block_state.replace(block_state_after_commit);

        tx_execution_results
    }
}
