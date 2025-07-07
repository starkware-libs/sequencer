use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dashmap::mapref::one::{Ref, RefMut};
use dashmap::DashMap;

use crate::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutorError,
    TransactionExecutorResult,
};
use crate::bouncer::Bouncer;
use crate::concurrency::fee_utils::complete_fee_transfer_flow;
use crate::concurrency::scheduler::{Scheduler, Task, TransactionStatus};
use crate::concurrency::versioned_state::{
    ThreadSafeVersionedState,
    VersionedState,
    VersionedStateError,
};
use crate::concurrency::TxIndex;
use crate::context::BlockContext;
use crate::metrics::{CALLS_RUNNING_NATIVE, TOTAL_CALLS};
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

#[derive(Debug, PartialEq)]
enum CommitResult {
    Success,
    NoRoomInBlock,
    ValidationFailed,
}

pub struct WorkerExecutor<S: StateReader> {
    pub scheduler: Scheduler,
    pub state: ThreadSafeVersionedState<S>,
    pub txs: DashMap<TxIndex, Arc<Transaction>>,
    pub n_txs: Mutex<usize>,
    pub execution_outputs: DashMap<TxIndex, ExecutionTaskOutput>,
    pub block_context: Arc<BlockContext>,
    pub bouncer: Arc<Mutex<Bouncer>>,
    pub execution_deadline: Option<Instant>,
    pub metrics: ConcurrencyMetrics,
}

impl<S: StateReader> WorkerExecutor<S> {
    pub fn new(
        state: ThreadSafeVersionedState<S>,
        txs: Vec<Transaction>,
        block_context: Arc<BlockContext>,
        bouncer: Arc<Mutex<Bouncer>>,
        execution_deadline: Option<Instant>,
    ) -> Self {
        let n_txs = txs.len();
        WorkerExecutor {
            scheduler: Scheduler::new(n_txs),
            state,
            txs: txs.into_iter().enumerate().map(|(i, tx)| (i, Arc::new(tx))).collect(),
            n_txs: Mutex::new(n_txs),
            execution_outputs: DashMap::new(),
            block_context,
            bouncer,
            execution_deadline,
            metrics: ConcurrencyMetrics::default(),
        }
    }

    // TODO(barak, 01/08/2024): Remove the `new` method or move it to test utils.
    pub fn initialize(
        state: S,
        txs: Vec<Transaction>,
        block_context: Arc<BlockContext>,
        bouncer: Arc<Mutex<Bouncer>>,
        execution_deadline: Option<Instant>,
    ) -> Self {
        let versioned_state = VersionedState::new(state);
        let chunk_state = ThreadSafeVersionedState::new(versioned_state);

        WorkerExecutor::new(chunk_state, txs, block_context, bouncer, execution_deadline)
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
                    if self.validate(tx_index, false).is_err() {
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

    pub fn add_txs(&self, txs: &[Transaction]) -> (TxIndex, TxIndex) {
        let mut n_txs_lock = self.n_txs.lock().expect("Failed to lock n_txs");

        let from_tx = *n_txs_lock;
        let n_new_txs = txs.len();
        for (i, tx) in txs.iter().enumerate() {
            self.txs.insert(from_tx + i, Arc::new(tx.clone()));
            // Notify the scheduler that a new transaction is available.
            self.scheduler.new_tx(from_tx + i);
        }
        let to_tx = from_tx + n_new_txs;
        *n_txs_lock = to_tx;
        (from_tx, to_tx)
    }

    /// Extracts the outputs of the completed transactions starting from `from_tx`.
    pub fn extract_execution_outputs(
        &self,
        from_tx: usize,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        let n_committed_txs = self.scheduler.get_n_committed_txs();
        (from_tx..n_committed_txs)
            .map(|tx_index| {
                let execution_output = self.extract_execution_output(tx_index);
                execution_output
                    .result
                    .map(|tx_execution_info| (tx_execution_info, execution_output.state_diff))
                    .map_err(TransactionExecutorError::from)
            })
            .collect()
    }

    /// Returns the transaction at the given index.
    /// Panics if the transaction does not exist.
    fn tx_at(&self, tx_index: TxIndex) -> Arc<Transaction> {
        self.txs.get(&tx_index).expect("Transaction missing").value().clone()
    }

    fn get_n_txs(&self) -> usize {
        *self.n_txs.lock().expect("Failed to lock n_txs")
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
                    CommitResult::ValidationFailed => {
                        tx_committer.uncommit();
                        return;
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
        // TODO(Yoni): is it necessary to use a transactional state here?
        let mut transactional_state =
            TransactionalState::create_transactional(&mut tx_versioned_state);
        let concurrency_mode = true;
        let tx = self.tx_at(tx_index);
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
        self.execution_outputs.insert(tx_index, execution_output_inner);
    }

    /// Validates the transaction at the given index and returns whether the transaction is valid.
    /// `commit_phase` should be `true` if the function is called during the commit phase.
    fn validate(&self, tx_index: TxIndex, commit_phase: bool) -> Result<bool, VersionedStateError> {
        self.metrics.count_validate();
        let tx_versioned_state = self.state.pin_version(tx_index);
        let Some(execution_output) = self.lock_execution_output_opt(tx_index) else {
            // If the execution output is missing, it means that the transaction was already
            // committed. This can happen if `commit_tx` precedes the `validation_index` run.
            // In this case, treat it as valid.
            assert!(!commit_phase, "Missing execution output in commit phase.");
            let status = self.scheduler.get_tx_status(tx_index);
            assert_eq!(
                status,
                TransactionStatus::Committed,
                "Missing execution output with tx_status={status:?}",
            );
            return Ok(true);
        };
        let reads = &execution_output.reads;
        let reads_valid = tx_versioned_state.validate_reads(reads)?;

        let aborted = !reads_valid && self.scheduler.try_validation_abort(tx_index, commit_phase);
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
        if !self.validate(tx_index, true)? {
            self.metrics.count_abort_in_commit();
            return Ok(CommitResult::ValidationFailed);
        }

        // Execution is final.
        let mut tx_versioned_state = self.state.pin_version(tx_index);
        let mut execution_output_refmut = self.lock_execution_output(tx_index);
        let execution_output = execution_output_refmut.value_mut();
        let mut tx_state_changes_keys = execution_output.state_diff.keys();

        if let Ok(tx_execution_info) = execution_output.result.as_mut() {
            let tx = self.tx_at(tx_index);
            let tx_context = self.block_context.to_tx_context(tx.as_ref());
            // Add the deleted sequencer balance key to the storage keys.
            let concurrency_mode = true;
            tx_state_changes_keys.update_sequencer_key_in_storage(
                &tx_context,
                tx_execution_info,
                concurrency_mode,
            );
            let execution_summary =
                tx_execution_info.summarize(&self.block_context.versioned_constants);

            let call_summary = execution_summary.call_summary.clone();
            // Ask the bouncer if there is room for the transaction in the block.
            let bouncer_result = self.bouncer.lock().expect("Bouncer lock failed.").try_update(
                &tx_versioned_state,
                &tx_state_changes_keys,
                &execution_summary,
                &tx_execution_info.summarize_builtins(),
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

            TOTAL_CALLS.increment(call_summary.n_calls);
            CALLS_RUNNING_NATIVE.increment(call_summary.n_calls_running_native);

            complete_fee_transfer_flow(
                &tx_context,
                tx_execution_info,
                &mut execution_output.state_diff,
                &mut tx_versioned_state,
                tx.as_ref(),
            );
            // Optimization: changing the sequencer balance storage cell does not trigger
            // (re-)validation of the next transactions.
        }

        Ok(CommitResult::Success)
    }

    /// Locks the execution output for the given transaction index.
    /// Panics if the execution output does not exist.
    pub fn lock_execution_output(
        &self,
        tx_index: TxIndex,
    ) -> RefMut<'_, TxIndex, ExecutionTaskOutput> {
        self.execution_outputs.get_mut(&tx_index).expect(EXECUTION_OUTPUTS_UNWRAP_ERROR)
    }

    /// Locks the execution output for the given transaction index.
    pub fn lock_execution_output_opt(
        &self,
        tx_index: TxIndex,
    ) -> Option<Ref<'_, TxIndex, ExecutionTaskOutput>> {
        self.execution_outputs.get(&tx_index)
    }

    /// Removes the execution output of the given transaction and returns it.
    pub fn extract_execution_output(&self, tx_index: TxIndex) -> ExecutionTaskOutput {
        self.execution_outputs.remove(&tx_index).expect(EXECUTION_OUTPUTS_UNWRAP_ERROR).1
    }
}

impl<U: UpdatableState> WorkerExecutor<U> {
    pub fn commit_chunk_and_recover_block_state(&self, n_committed_txs: usize) -> U {
        let (abort_counter, abort_in_commit_counter, execute_counter, validate_counter) =
            self.metrics.get_metrics();
        let n_txs = self.get_n_txs();
        log::debug!(
            "Concurrent execution done. Number of transactions: {n_txs}; Committed chunk size: \
             {n_committed_txs}; Execute counter: {execute_counter}; Validate counter: \
             {validate_counter}; Abort counter: {abort_counter}; Abort in commit counter: \
             {abort_in_commit_counter}"
        );

        self.state.into_inner_state().commit_chunk_and_recover_block_state(n_committed_txs)
    }
}
