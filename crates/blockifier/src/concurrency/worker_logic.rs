use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::blockifier::transaction_executor::TransactionExecutorError;
use crate::bouncer::Bouncer;
use crate::concurrency::fee_utils::complete_fee_transfer_flow;
use crate::concurrency::scheduler::{Scheduler, Task};
use crate::concurrency::utils::lock_mutex_in_array;
use crate::concurrency::versioned_state::{
    ThreadSafeVersionedState,
    VersionedState,
    VersionedStateError,
};
use crate::concurrency::TxIndex;
use crate::context::BlockContext;
use crate::state::cached_state::{ContractClassMapping, StateMaps, TransactionalState};
use crate::state::state_api::{StateReader, UpdatableState};
use crate::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use crate::transaction::transaction_execution::Transaction;
use crate::transaction::transactions::ExecutableTransaction;

#[cfg(test)]
#[path = "worker_logic_test.rs"]
pub mod test;

const EXECUTION_OUTPUTS_UNWRAP_ERROR: &str = "Execution task outputs should not be None.";

#[derive(Debug)]
pub struct ExecutionTaskOutput {
    pub reads: StateMaps,
    pub state_diff: StateMaps,
    pub contract_classes: ContractClassMapping,
    pub result: TransactionExecutionResult<TransactionExecutionInfo>,
}

#[derive(Default)]
pub struct ConcurrencyMetrics {
    abort_counter: AtomicUsize,
    abort_in_commit_counter: AtomicUsize,
    execute_counter: AtomicUsize,
    validate_counter: AtomicUsize,
}

impl ConcurrencyMetrics {
    pub fn count_abort(&self) {
        self.abort_counter.fetch_add(1, Ordering::Relaxed);
    }
    pub fn count_abort_in_commit(&self) {
        self.abort_in_commit_counter.fetch_add(1, Ordering::Relaxed);
    }
    pub fn count_execute(&self) {
        self.execute_counter.fetch_add(1, Ordering::Relaxed);
    }
    pub fn count_validate(&self) {
        self.validate_counter.fetch_add(1, Ordering::Relaxed);
    }
    pub fn get_metrics(&self) -> (usize, usize, usize, usize) {
        (
            self.abort_counter.load(Ordering::Relaxed),
            self.abort_in_commit_counter.load(Ordering::Relaxed),
            self.execute_counter.load(Ordering::Relaxed),
            self.validate_counter.load(Ordering::Relaxed),
        )
    }
}

enum CommitResult {
    Success,
    NoRoomInBlock,
}

pub struct WorkerExecutor<S: StateReader> {
    pub scheduler: Scheduler,
    pub state: ThreadSafeVersionedState<S>,
    pub chunk: Vec<Transaction>,
    pub execution_outputs: Box<[Mutex<Option<ExecutionTaskOutput>>]>,
    pub block_context: Arc<BlockContext>,
    pub bouncer: Arc<Mutex<Bouncer>>,
    pub execution_deadline: Option<Instant>,
    pub metrics: ConcurrencyMetrics,
}

impl<S: StateReader> WorkerExecutor<S> {
    pub fn new(
        state: ThreadSafeVersionedState<S>,
        chunk: Vec<Transaction>,
        block_context: Arc<BlockContext>,
        bouncer: Arc<Mutex<Bouncer>>,
    ) -> Self {
        let scheduler = Scheduler::new(chunk.len());
        let execution_outputs =
            std::iter::repeat_with(|| Mutex::new(None)).take(chunk.len()).collect();
        let metrics = ConcurrencyMetrics::default();

        WorkerExecutor {
            scheduler,
            state,
            chunk,
            execution_outputs,
            block_context,
            bouncer,
            execution_deadline: None,
            metrics,
        }
    }

    // TODO(barak, 01/08/2024): Remove the `new` method or move it to test utils.
    pub fn initialize(
        state: S,
        chunk: Vec<Transaction>,
        block_context: Arc<BlockContext>,
        bouncer: Arc<Mutex<Bouncer>>,
        execution_deadline: Option<Instant>,
    ) -> Self {
        let versioned_state = VersionedState::new(state);
        let chunk_state = ThreadSafeVersionedState::new(versioned_state);
        let scheduler = Scheduler::new(chunk.len());
        let execution_outputs =
            std::iter::repeat_with(|| Mutex::new(None)).take(chunk.len()).collect();
        let metrics = ConcurrencyMetrics::default();

        WorkerExecutor {
            scheduler,
            state: chunk_state,
            chunk,
            execution_outputs,
            block_context,
            bouncer,
            execution_deadline,
            metrics,
        }
    }

    pub fn run(&self) {
        loop {
            if let Some(deadline) = self.execution_deadline {
                if Instant::now() > deadline {
                    log::debug!("Execution timed out.");
                    // TODO(Yoni): Reconsider the location of this check.
                    self.scheduler.halt();
                    break;
                }
            }
            self.commit_while_possible();

            match self.scheduler.next_task() {
                Task::ExecutionTask(tx_index) => {
                    self.execute(tx_index);
                }
                Task::ValidationTask(tx_index) => {
                    if self.validate(tx_index).is_err() {
                        assert!(self.scheduler.done());
                        break;
                    }
                }
                Task::NoTaskAvailable => {
                    // There's no available task at the moment; sleep for a bit to save CPU power.
                    // (since busy-looping might damage performance when using hyper-threads).
                    thread::sleep(Duration::from_micros(1));
                }
                Task::AskForTask => continue,
                Task::Done => break,
            };
        }
    }

    fn commit_while_possible(&self) {
        if let Some(mut tx_committer) = self.scheduler.try_enter_commit_phase() {
            while let Some(tx_index) = tx_committer.try_commit() {
                let commit_result = self.commit_tx(tx_index).unwrap_or_else(|_| {
                    panic!("Commit transaction should not be called after clearing the state.");
                });
                match commit_result {
                    CommitResult::Success => {}
                    CommitResult::NoRoomInBlock => {
                        tx_committer.uncommit();
                        self.scheduler.halt();
                    }
                }
            }
        }
    }

    fn execute(&self, tx_index: TxIndex) {
        self.metrics.count_execute();
        self.execute_tx(tx_index);
        self.scheduler.finish_execution(tx_index)
    }

    fn execute_tx(&self, tx_index: TxIndex) {
        let mut tx_versioned_state = self.state.pin_version(tx_index);
        let tx = &self.chunk[tx_index];
        // TODO(Yoni): is it necessary to use a transactional state here?
        let mut transactional_state =
            TransactionalState::create_transactional(&mut tx_versioned_state);
        let concurrency_mode = true;
        let execution_result =
            tx.execute_raw(&mut transactional_state, &self.block_context, concurrency_mode);

        // Update the versioned state and store the transaction execution output.
        let execution_output_inner = match execution_result {
            Ok(_) => {
                let tx_reads_writes = transactional_state.cache.take();
                let state_diff = tx_reads_writes.to_state_diff().state_maps;
                let contract_classes = transactional_state.class_hash_to_class.take();
                tx_versioned_state.apply_writes(&state_diff, &contract_classes);
                ExecutionTaskOutput {
                    reads: tx_reads_writes.initial_reads,
                    state_diff,
                    contract_classes,
                    result: execution_result,
                }
            }
            Err(_) => ExecutionTaskOutput {
                reads: transactional_state.cache.take().initial_reads,
                // Failed transaction - ignore the writes.
                state_diff: StateMaps::default(),
                contract_classes: HashMap::default(),
                result: execution_result,
            },
        };
        let mut execution_output = lock_mutex_in_array(&self.execution_outputs, tx_index);
        *execution_output = Some(execution_output_inner);
    }

    /// Validates the transaction at the given index and returns whether the transaction is valid.
    fn validate(&self, tx_index: TxIndex) -> Result<bool, VersionedStateError> {
        self.metrics.count_validate();
        let tx_versioned_state = self.state.pin_version(tx_index);
        let execution_output = lock_mutex_in_array(&self.execution_outputs, tx_index);
        let execution_output = execution_output.as_ref().expect(EXECUTION_OUTPUTS_UNWRAP_ERROR);
        let reads = &execution_output.reads;
        let reads_valid = tx_versioned_state.validate_reads(reads)?;

        let aborted = !reads_valid && self.scheduler.try_validation_abort(tx_index);
        if aborted {
            self.metrics.count_abort();
            tx_versioned_state
                .delete_writes(&execution_output.state_diff, &execution_output.contract_classes)?;
            self.scheduler.finish_abort(tx_index);
        }
        Ok(reads_valid)
    }

    /// Commits a transaction. The commit process is as follows:
    /// 1) Validate the read set.
    ///     * If validation failed, delete the transaction writes and (re-)execute it.
    ///     * Else (validation succeeded), no need to re-execute.
    /// 2) Execution is final.
    ///     * If execution succeeded, ask the bouncer if there is room for the transaction in the
    ///       block.
    ///         - If there is room, fix the call info, update the sequencer balance and commit the
    ///           transaction.
    ///         - Else (no room), do not commit. The block should be closed without the transaction.
    ///     * Else (execution failed), commit the transaction without fixing the call info or
    ///       updating the sequencer balance.
    fn commit_tx(&self, tx_index: TxIndex) -> Result<CommitResult, VersionedStateError> {
        let execution_output = lock_mutex_in_array(&self.execution_outputs, tx_index);
        let execution_output_ref = execution_output.as_ref().expect(EXECUTION_OUTPUTS_UNWRAP_ERROR);
        let reads = &execution_output_ref.reads;

        let mut tx_versioned_state = self.state.pin_version(tx_index);
        let reads_valid = tx_versioned_state.validate_reads(reads)?;

        // First, re-validate the transaction.
        if !reads_valid {
            // Revalidate failed: re-execute the transaction.
            self.metrics.count_abort_in_commit();
            tx_versioned_state.delete_writes(
                &execution_output_ref.state_diff,
                &execution_output_ref.contract_classes,
            )?;
            // Release the execution output lock as it is acquired in execution (avoid dead-lock).
            drop(execution_output);

            // TODO(Yoni): avoid re-executing in the commit phase.
            self.execute_tx(tx_index);
            self.scheduler.finish_execution_during_commit(tx_index);

            let execution_output = lock_mutex_in_array(&self.execution_outputs, tx_index);
            let read_set = &execution_output.as_ref().expect(EXECUTION_OUTPUTS_UNWRAP_ERROR).reads;
            // Another validation after the re-execution for sanity check.
            assert!(tx_versioned_state.validate_reads(read_set)?);
        } else {
            // Release the execution output lock, since it is has been released in the other flow.
            drop(execution_output);
        }

        // Execution is final.
        let mut execution_output = lock_mutex_in_array(&self.execution_outputs, tx_index);
        let execution_output = execution_output.as_mut().expect(EXECUTION_OUTPUTS_UNWRAP_ERROR);
        let mut tx_state_changes_keys = execution_output.state_diff.keys();

        if let Ok(tx_execution_info) = execution_output.result.as_mut() {
            let tx_context = self.block_context.to_tx_context(&self.chunk[tx_index]);
            // Add the deleted sequencer balance key to the storage keys.
            let concurrency_mode = true;
            tx_state_changes_keys.update_sequencer_key_in_storage(
                &tx_context,
                tx_execution_info,
                concurrency_mode,
            );
            // Ask the bouncer if there is room for the transaction in the block.
            let bouncer_result = self.bouncer.lock().expect("Bouncer lock failed.").try_update(
                &tx_versioned_state,
                &tx_state_changes_keys,
                &tx_execution_info.summarize(&self.block_context.versioned_constants),
                &tx_execution_info.receipt.resources,
                &self.block_context.versioned_constants,
            );
            if let Err(error) = bouncer_result {
                match error {
                    TransactionExecutorError::BlockFull => return Ok(CommitResult::NoRoomInBlock),
                    _ => {
                        // TODO(Avi, 01/07/2024): Consider propagating the error.
                        panic!("Bouncer update failed. {error:?}: {error}");
                    }
                }
            }
            complete_fee_transfer_flow(
                &tx_context,
                tx_execution_info,
                &mut execution_output.state_diff,
                &mut tx_versioned_state,
                &self.chunk[tx_index],
            );
            // Optimization: changing the sequencer balance storage cell does not trigger
            // (re-)validation of the next transactions.
        }

        Ok(CommitResult::Success)
    }
}

impl<U: UpdatableState> WorkerExecutor<U> {
    pub fn commit_chunk_and_recover_block_state(&self, n_committed_txs: usize) -> U {
        self.state.into_inner_state().commit_chunk_and_recover_block_state(n_committed_txs)
    }
}
