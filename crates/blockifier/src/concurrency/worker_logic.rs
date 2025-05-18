use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use dashmap::mapref::one::RefMut;
use dashmap::DashMap;

use super::versioned_state::VersionedState;
use crate::blockifier::transaction_executor::TransactionExecutorError;
use crate::bouncer::Bouncer;
use crate::concurrency::fee_utils::complete_fee_transfer_flow;
use crate::concurrency::scheduler::{Scheduler, Task};
use crate::concurrency::versioned_state::ThreadSafeVersionedState;
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

pub struct WorkerExecutor<S: StateReader> {
    pub scheduler: Scheduler,
    pub state: ThreadSafeVersionedState<S>,
    pub transactions: DashMap<TxIndex, Arc<Transaction>>,
    pub n_transactions: Mutex<usize>,
    pub execution_outputs: DashMap<TxIndex, ExecutionTaskOutput>,
    pub block_context: Arc<BlockContext>,
    pub bouncer: Arc<Mutex<Bouncer>>,
    pub metrics: ConcurrencyMetrics,
}

impl<S: StateReader> WorkerExecutor<S> {
    pub fn new(
        state: ThreadSafeVersionedState<S>,
        chunk_vec: Vec<Transaction>,
        block_context: Arc<BlockContext>,
        bouncer: Arc<Mutex<Bouncer>>,
    ) -> Self {
        let n_transactions = Mutex::new(chunk_vec.len());
        WorkerExecutor {
            scheduler: Scheduler::new(),
            state,
            transactions: chunk_vec
                .into_iter()
                .enumerate()
                .map(|(i, tx)| (i, Arc::new(tx)))
                .collect(),
            n_transactions,
            execution_outputs: DashMap::new(),
            block_context,
            bouncer,
            metrics: ConcurrencyMetrics::default(),
        }
    }

    // TODO(barak, 01/08/2024): Remove the `new` method or move it to test utils.
    pub fn initialize(
        state: S,
        transactions: Vec<Transaction>,
        block_context: Arc<BlockContext>,
        bouncer: Arc<Mutex<Bouncer>>,
    ) -> Self {
        let versioned_state = VersionedState::new(state);
        let chunk_state = ThreadSafeVersionedState::new(versioned_state);

        WorkerExecutor::new(chunk_state, transactions, block_context, bouncer)
    }

    pub fn run(&self) {
        let mut task = Task::AskForTask;
        loop {
            self.commit_while_possible();
            task = match task {
                Task::ExecutionTask(tx_index) => {
                    self.execute(tx_index);
                    Task::AskForTask
                }
                Task::ValidationTask(tx_index) => self.validate(tx_index),
                Task::AskForTask => self.scheduler.next_task(),
                Task::Done => break,
            };
        }
    }

    /// Returns the transaction at the given index.
    /// Panics if the transaction does not exist.
    fn tx_at(&self, tx_index: TxIndex) -> Arc<Transaction> {
        self.transactions.get(&tx_index).expect("Transaction missing").value().clone()
    }

    fn get_n_transactions(&self) -> usize {
        *self.n_transactions.lock().expect("Failed to lock n_transactions")
    }

    fn commit_while_possible(&self) {
        if let Some(mut tx_committer) = self.scheduler.try_enter_commit_phase() {
            while let Some(tx_index) = tx_committer.try_commit() {
                let commit_succeeded = self.commit_tx(tx_index);
                if !commit_succeeded {
                    tx_committer.abort_task_and_halt_scheduler();
                } else if tx_index == self.get_n_transactions() - 1 {
                    self.scheduler.halt();
                }
            }
        }
    }

    fn execute(&self, tx_index: TxIndex) {
        if tx_index < self.get_n_transactions() {
            self.metrics.count_execute();
            self.execute_tx(tx_index);
            self.scheduler.finish_execution(tx_index)
        } else {
            // There's no available task at the moment; sleep for a bit to prevent busy-waiting.
            thread::sleep(Duration::from_micros(1));

            self.scheduler.mark_missing_task(tx_index);
        }
    }

    fn execute_tx(&self, tx_index: TxIndex) {
        let mut tx_versioned_state = self.state.pin_version(tx_index);
        // TODO(Yoni): is it necessary to use a transactional state here?
        let mut transactional_state =
            TransactionalState::create_transactional(&mut tx_versioned_state);
        let concurrency_mode = true;
        let execution_result = self.tx_at(tx_index).execute_raw(
            &mut transactional_state,
            &self.block_context,
            concurrency_mode,
        );

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

    fn validate(&self, tx_index: TxIndex) -> Task {
        self.metrics.count_validate();
        let tx_versioned_state = self.state.pin_version(tx_index);
        let execution_output = self.lock_execution_output(tx_index);
        let reads = &execution_output.reads;
        let reads_valid = tx_versioned_state.validate_reads(reads);

        let aborted = !reads_valid && self.scheduler.try_validation_abort(tx_index);
        if aborted {
            self.metrics.count_abort();
            tx_versioned_state
                .delete_writes(&execution_output.state_diff, &execution_output.contract_classes);
            self.scheduler.finish_abort(tx_index)
        } else {
            Task::AskForTask
        }
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
    fn commit_tx(&self, tx_index: TxIndex) -> bool {
        let execution_output = self.lock_execution_output(tx_index);
        let reads = &execution_output.reads;

        let mut tx_versioned_state = self.state.pin_version(tx_index);
        let reads_valid = tx_versioned_state.validate_reads(reads);

        // First, re-validate the transaction.
        if !reads_valid {
            // Revalidate failed: re-execute the transaction.
            self.metrics.count_abort_in_commit();
            tx_versioned_state
                .delete_writes(&execution_output.state_diff, &execution_output.contract_classes);
            // Release the execution output lock as it is acquired in execution (avoid dead-lock).
            drop(execution_output);

            self.execute_tx(tx_index);
            self.scheduler.finish_execution_during_commit(tx_index);

            let execution_output = self.lock_execution_output(tx_index);
            let read_set = &execution_output.reads;
            // Another validation after the re-execution for sanity check.
            assert!(tx_versioned_state.validate_reads(read_set));
        } else {
            // Release the execution output lock, since it is has been released in the other flow.
            drop(execution_output);
        }

        // Execution is final.
        let mut execution_output_refmut = self.lock_execution_output(tx_index);
        let execution_output = execution_output_refmut.value_mut();
        let mut tx_state_changes_keys = execution_output.state_diff.keys();

        if let Ok(tx_execution_info) = execution_output.result.as_mut() {
            let tx = &*self.tx_at(tx_index);
            let tx_context = self.block_context.to_tx_context(tx);
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
                    TransactionExecutorError::BlockFull => return false,
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
                tx,
            );
            // Optimization: changing the sequencer balance storage cell does not trigger
            // (re-)validation of the next transactions.
        }

        true
    }

    /// Locks the execution output for the given transaction index.
    /// Panics if the execution output does not exist.
    pub fn lock_execution_output(
        &self,
        tx_index: TxIndex,
    ) -> RefMut<'_, TxIndex, ExecutionTaskOutput> {
        self.execution_outputs.get_mut(&tx_index).expect(EXECUTION_OUTPUTS_UNWRAP_ERROR)
    }

    /// Removes the execution output of the given transaction and returns it.
    pub fn extract_execution_output(&self, tx_index: TxIndex) -> ExecutionTaskOutput {
        self.execution_outputs.remove(&tx_index).expect(EXECUTION_OUTPUTS_UNWRAP_ERROR).1
    }
}

impl<U: UpdatableState> WorkerExecutor<U> {
    pub fn commit_chunk_and_recover_block_state(&self, n_committed_txs: usize) -> U {
        self.state.into_inner_state().commit_chunk_and_recover_block_state(n_committed_txs)
    }
}
