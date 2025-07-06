use std::mem;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use apollo_infra_utils::tracing::LogCompatibleToStringExt;
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use starknet_api::block::BlockHashAndNumber;
use thiserror::Error;

use crate::blockifier::block::pre_process_block;
use crate::blockifier::config::TransactionExecutorConfig;
use crate::bouncer::{Bouncer, BouncerWeights, CasmHashComputationData};
use crate::concurrency::worker_logic::WorkerExecutor;
use crate::concurrency::worker_pool::WorkerPool;
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

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug)]
pub struct BlockExecutionSummary {
    pub state_diff: CommitmentStateDiff,
    pub compressed_state_diff: Option<CommitmentStateDiff>,
    pub bouncer_weights: BouncerWeights,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
}

/// A transaction executor, used for building a single block.
pub struct TransactionExecutor<S: StateReader> {
    pub block_context: Arc<BlockContext>,
    pub bouncer: Arc<Mutex<Bouncer>>,
    // Note: this config must not affect the execution result (e.g. state diff and traces).
    pub config: TransactionExecutorConfig,

    // State-related fields.
    // The transaction executor operates at the block level. In concurrency mode, it moves the
    // block state to the worker executor - operating at the chunk level - and gets it back after
    // committing the chunk. The block state is wrapped with an Option<_> to allow setting it to
    // `None` while it is moved to the worker executor.
    pub block_state: Option<CachedState<S>>,

    pub worker_pool: Option<Arc<WorkerPool<CachedState<S>>>>,
}

impl<S: StateReader> TransactionExecutor<S> {
    /// Performs pre-processing required for block building before creating the executor.
    pub fn pre_process_and_create(
        initial_state_reader: S,
        block_context: BlockContext,
        old_block_number_and_hash: Option<BlockHashAndNumber>,
        config: TransactionExecutorConfig,
    ) -> StateResult<Self> {
        Self::pre_process_and_create_with_pool(
            initial_state_reader,
            block_context,
            old_block_number_and_hash,
            config,
            None,
        )
    }

    /// Performs pre-processing required for block building before creating the executor.
    pub fn pre_process_and_create_with_pool(
        initial_state_reader: S,
        block_context: BlockContext,
        old_block_number_and_hash: Option<BlockHashAndNumber>,
        config: TransactionExecutorConfig,
        worker_pool: Option<Arc<WorkerPool<CachedState<S>>>>,
    ) -> StateResult<Self> {
        let mut block_state = CachedState::new(initial_state_reader);
        pre_process_block(
            &mut block_state,
            old_block_number_and_hash,
            block_context.block_info().block_number,
            &block_context.versioned_constants.os_constants,
        )?;
        Ok(Self::new_with_pool(block_state, block_context, config, worker_pool))
    }

    // TODO(Yoni): consider making this c-tor private.
    pub fn new(
        block_state: CachedState<S>,
        block_context: BlockContext,
        config: TransactionExecutorConfig,
    ) -> Self {
        Self::new_with_pool(block_state, block_context, config, None)
    }

    fn new_with_pool(
        block_state: CachedState<S>,
        block_context: BlockContext,
        config: TransactionExecutorConfig,
        worker_pool: Option<Arc<WorkerPool<CachedState<S>>>>,
    ) -> Self {
        let bouncer_config = block_context.bouncer_config.clone();
        // Note: the state might not be empty even at this point; it is the creator's
        // responsibility to tune the bouncer according to pre and post block process.
        Self {
            block_context: block_context.into(),
            bouncer: Mutex::new(Bouncer::new(bouncer_config)).into(),
            config,
            block_state: Some(block_state),
            worker_pool,
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
                lock_bouncer(&self.bouncer).try_update(
                    &transactional_state,
                    &tx_state_changes_keys,
                    &tx_execution_info.summarize(&self.block_context.versioned_constants),
                    &tx_execution_info.summarize_builtins(),
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
        finalize_block(
            &self.bouncer,
            self.block_state.as_mut().expect(BLOCK_STATE_ACCESS_ERR),
            &self.block_context,
        )
    }

    #[cfg(feature = "reexecution")]
    pub fn non_consuming_finalize(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        finalize_block(
            &self.bouncer,
            self.block_state.as_mut().expect(BLOCK_STATE_ACCESS_ERR),
            &self.block_context,
        )
    }
}

fn lock_bouncer(bouncer: &Arc<Mutex<Bouncer>>) -> MutexGuard<'_, Bouncer> {
    bouncer.lock().expect("Bouncer lock failed.")
}

/// Finalizes the creation of a block.
/// Returns the state diff and the block weights.
pub(crate) fn finalize_block<S: StateReader>(
    bouncer: &Arc<Mutex<Bouncer>>,
    block_state: &mut CachedState<S>,
    block_context: &BlockContext,
) -> TransactionExecutorResult<BlockExecutionSummary> {
    log::debug!("Final block weights: {:?}.", lock_bouncer(bouncer).get_bouncer_weights());
    let alias_contract_address = block_context
        .versioned_constants
        .os_constants
        .os_contract_addresses
        .alias_contract_address();
    if block_context.versioned_constants.enable_stateful_compression {
        allocate_aliases_in_storage(block_state, alias_contract_address)?;
    }
    let state_diff = block_state.to_state_diff()?.state_maps;
    let compressed_state_diff = if block_context.versioned_constants.enable_stateful_compression {
        Some(compress(&state_diff, block_state, alias_contract_address)?.into())
    } else {
        None
    };

    // Take CasmHashComputationData from bouncer,
    // and verify that class hashes are the same.
    let mut bouncer = lock_bouncer(bouncer);
    let casm_hash_computation_data_sierra_gas =
        mem::take(bouncer.get_mut_casm_hash_computation_data_sierra_gas());
    let casm_hash_computation_data_proving_gas =
        mem::take(bouncer.get_mut_casm_hash_computation_data_proving_gas());
    assert_eq!(
        casm_hash_computation_data_sierra_gas
            .class_hash_to_casm_hash_computation_gas
            .keys()
            .collect::<std::collections::HashSet<_>>(),
        casm_hash_computation_data_proving_gas
            .class_hash_to_casm_hash_computation_gas
            .keys()
            .collect::<std::collections::HashSet<_>>()
    );

    Ok(BlockExecutionSummary {
        state_diff: state_diff.into(),
        compressed_state_diff,
        bouncer_weights: *bouncer.get_bouncer_weights(),
        casm_hash_computation_data_sierra_gas,
        casm_hash_computation_data_proving_gas,
    })
}

impl<S: StateReader + Send + Sync> TransactionExecutor<S> {
    /// Executes the given transactions on the state maintained by the executor.
    ///
    /// # Arguments:
    /// * `txs` - A slice of transactions to be executed.
    /// * `execution_deadline` - An optional deadline for the execution.
    ///
    /// Returns a vector of `TransactionExecutorResult<TransactionExecutionOutput>`, containing the
    /// execution results for each transaction. The execution may stop early if the block becomes
    /// full.
    pub fn execute_txs(
        &mut self,
        txs: &[Transaction],
        execution_deadline: Option<Instant>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>>
    where
        S: 'static,
    {
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
                 equals {chunk_size:?} "
            );
            assert!(
                n_workers > 0,
                "When running transactions concurrently the number of workers must be greater \
                 than 0. It equals {n_workers:?} "
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
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>>
    where
        S: 'static,
    {
        let block_state = self.block_state.take().expect("The block state should be `Some`.");

        let worker_executor = Arc::new(WorkerExecutor::initialize(
            block_state,
            // We need to clone the transactions so that ownership can be shared between threads,
            // that will live longer than the current function.
            // TODO(lior): Move the transactions instead of cloning them.
            chunk.to_vec(),
            self.block_context.clone(),
            self.bouncer.clone(),
            execution_deadline,
        ));

        if let Some(worker_pool) = &mut self.worker_pool {
            worker_pool.run_and_wait(worker_executor.clone(), chunk.len());
        } else {
            // If a pool is not given, create a new pool and wait for it to finish.
            let worker_pool = WorkerPool::start(&self.config.get_worker_pool_config());
            worker_pool.run_and_wait(worker_executor.clone(), chunk.len());
            worker_pool.join();
        }

        let tx_execution_results = worker_executor.extract_execution_outputs(0);
        let n_committed_txs = tx_execution_results.len();

        let block_state_after_commit =
            worker_executor.commit_chunk_and_recover_block_state(n_committed_txs);
        self.block_state.replace(block_state_after_commit);

        tx_execution_results
    }
}
