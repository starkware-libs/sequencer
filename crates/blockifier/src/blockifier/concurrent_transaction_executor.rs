use std::mem;
use std::sync::{Arc, Mutex};

use starknet_api::block::BlockHashAndNumber;

use crate::blockifier::block::pre_process_block;
use crate::blockifier::transaction_executor::{BlockExecutionSummary, TransactionExecutorResult};
use crate::bouncer::Bouncer;
use crate::concurrency::worker_logic::WorkerExecutor;
use crate::concurrency::worker_pool::WorkerPool;
use crate::context::BlockContext;
use crate::state::cached_state::CachedState;
use crate::state::state_api::{StateReader, StateResult};
use crate::state::stateful_compression::{allocate_aliases_in_storage, compress};
use crate::blockifier::transaction_executor::TransactionExecutionOutput;
use crate::transaction::transaction_execution::Transaction;

pub struct ConcurrentTransactionExecutor<S: StateReader> {
    // TODO: Remove `pub`.
    pub worker_executor: Arc<WorkerExecutor<CachedState<S>>>,
}

impl<S: StateReader + Send + 'static> ConcurrentTransactionExecutor<S> {
    pub fn start_block(
        initial_state_reader: S,
        block_context: BlockContext,
        old_block_number_and_hash: Option<BlockHashAndNumber>,
        worker_pool: Arc<WorkerPool<CachedState<S>>>,
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
        ));
        worker_pool.run(worker_executor.clone());

        Ok(Self { worker_executor })
    }

    pub fn add_transactions_and_wait(
        &mut self,
        txs: &[Transaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        self.worker_executor.add_transactions_and_wait(txs)
    }

    pub fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        let worker_executor = &self.worker_executor;
        log::debug!(
            "Final block weights: {:?}.",
            worker_executor.bouncer.lock().expect("Bouncer lock failed.").get_accumulated_weights()
        );
        // TODO: Get n_committed_txs from the caller.
        let n_committed_txs = worker_executor.scheduler.get_n_committed_txs();
        let mut state_after_block =
            worker_executor.commit_chunk_and_recover_block_state(n_committed_txs);
        let alias_contract_address = self
            .worker_executor
            .block_context
            .versioned_constants
            .os_constants
            .os_contract_addresses
            .alias_contract_address();
        if worker_executor.block_context.versioned_constants.enable_stateful_compression {
            allocate_aliases_in_storage(&mut state_after_block, alias_contract_address)?;
        }
        let state_diff = state_after_block.to_state_diff()?.state_maps;
        let compressed_state_diff =
            if worker_executor.block_context.versioned_constants.enable_stateful_compression {
                Some(compress(&state_diff, &mut state_after_block, alias_contract_address)?.into())
            } else {
                None
            };

        let mut bouncer = worker_executor.bouncer.lock().expect("Bouncer lock failed.");
        Ok(BlockExecutionSummary {
            state_diff: state_diff.into(),
            compressed_state_diff,
            bouncer_weights: *bouncer.get_accumulated_weights(),
            casm_hash_computation_data: mem::take(&mut bouncer.casm_hash_computation_data),
        })
    }
}
