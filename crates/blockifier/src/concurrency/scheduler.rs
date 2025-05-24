use std::cmp::min;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};

use dashmap::mapref::one::RefMut;
use dashmap::DashMap;

use crate::concurrency::TxIndex;

#[cfg(test)]
#[path = "scheduler_test.rs"]
pub mod test;

#[cfg(test)]
#[path = "flow_test.rs"]
pub mod flow_test;

pub struct TransactionCommitter<'a> {
    scheduler: &'a Scheduler,
    commit_index_guard: MutexGuard<'a, usize>,
}

impl<'a> TransactionCommitter<'a> {
    pub fn new(scheduler: &'a Scheduler, commit_index_guard: MutexGuard<'a, usize>) -> Self {
        Self { scheduler, commit_index_guard }
    }

    /// Tries to commit the next uncommitted transaction in the chunk. Returns the index of the
    /// transaction to commit if successful, or None if the transaction is not yet executed.
    pub fn try_commit(&mut self) -> Option<usize> {
        if self.scheduler.done() {
            return None;
        };

        let mut status = self.scheduler.lock_tx_status(*self.commit_index_guard);
        if *status != TransactionStatus::Executed {
            return None;
        }
        *status = TransactionStatus::Committed;
        *self.commit_index_guard += 1;
        Some(*self.commit_index_guard - 1)
    }

    /// Decrements the commit index to indicate that the final transaction to commit has been
    /// excluded from the block.
    pub fn uncommit(&mut self) {
        assert!(*self.commit_index_guard > 0, "Commit index underflow.");
        *self.commit_index_guard -= 1;
    }
}

#[derive(Debug, Default)]
pub struct Scheduler {
    execution_index: AtomicUsize,
    validation_index: AtomicUsize,
    /// The index of the next transaction to commit.
    commit_index: Mutex<usize>,
    tx_statuses: DashMap<TxIndex, TransactionStatus>,
    /// Set to true when calling `halt()`. This will cause all threads to exit their main loops.
    done_marker: AtomicBool,
}

impl Scheduler {
    pub fn new(n_txs: usize) -> Scheduler {
        let tx_statuses = DashMap::new();
        for i in 0..n_txs {
            tx_statuses.insert(i, TransactionStatus::ReadyToExecute);
        }
        Scheduler {
            execution_index: AtomicUsize::new(0),
            validation_index: AtomicUsize::new(0),
            commit_index: Mutex::new(0),
            tx_statuses,
            done_marker: AtomicBool::new(false),
        }
    }

    pub fn next_task(&self) -> Task {
        if self.done() {
            return Task::Done;
        }

        let index_to_validate = self.validation_index.load(Ordering::Acquire);
        let index_to_execute = self.execution_index.load(Ordering::Acquire);

        if self.get_tx_status(min(index_to_validate, index_to_execute))
            == TransactionStatus::Missing
        {
            return Task::NoTaskAvailable;
        }

        if index_to_validate < index_to_execute {
            if let Some(tx_index) = self.next_version_to_validate() {
                return Task::ValidationTask(tx_index);
            }
        }

        if let Some(tx_index) = self.next_version_to_execute() {
            return Task::ExecutionTask(tx_index);
        }

        Task::AskForTask
    }

    /// Marks that a new transaction is available for processing.
    ///
    /// Note: transactions must be added in order, without holes.
    pub fn new_tx(&self, tx_index: TxIndex) {
        let mut status = self.lock_tx_status(tx_index);
        assert_eq!(
            *status,
            TransactionStatus::Missing,
            "Transaction {tx_index} is not marked as missing."
        );
        *status = TransactionStatus::ReadyToExecute;
    }

    /// Updates the Scheduler that an execution task has been finished and triggers the creation of
    /// new tasks accordingly: schedules validation for the current and higher transactions, if not
    /// already scheduled.
    pub fn finish_execution(&self, tx_index: TxIndex) {
        self.set_executed_status(tx_index);
        self.decrease_validation_index(tx_index);
    }

    /// Marks the given transaction as `Aborting` if the current status is `Executed`.
    /// `commit_phase` should be `true` if the function is called during the commit phase.
    pub fn try_validation_abort(&self, tx_index: TxIndex, commit_phase: bool) -> bool {
        let mut status = self.lock_tx_status(tx_index);
        if commit_phase {
            assert_eq!(
                *status,
                TransactionStatus::Committed,
                "Unexpected status during commit phase: {:?}",
                *status
            );
            *status = TransactionStatus::Aborting;
            return true;
        }

        if *status == TransactionStatus::Executed {
            *status = TransactionStatus::Aborting;
            return true;
        }
        false
    }

    /// Updates the Scheduler that a validation task has aborted.
    /// Decreases the execution index to ensure that the transaction will be re-executed.
    pub fn finish_abort(&self, tx_index: TxIndex) {
        self.set_ready_status(tx_index);
        self.decrease_execution_index(tx_index);
    }

    /// Tries to takes the lock on the commit index. Returns a `TransactionCommitter` if successful,
    /// or None if the lock is already taken.
    pub fn try_enter_commit_phase(&self) -> Option<TransactionCommitter<'_>> {
        match self.commit_index.try_lock() {
            Ok(guard) => Some(TransactionCommitter::new(self, guard)),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(error)) => {
                panic!("Commit index is poisoned. Data: {:?}.", *error.get_ref())
            }
        }
    }

    pub fn get_n_committed_txs(&self) -> usize {
        *self.commit_index.lock().unwrap()
    }

    pub fn halt(&self) {
        self.done_marker.store(true, Ordering::Release);
    }

    fn lock_tx_status(&self, tx_index: TxIndex) -> RefMut<'_, TxIndex, TransactionStatus> {
        self.tx_statuses.entry(tx_index).or_insert(TransactionStatus::Missing)
    }

    fn set_executed_status(&self, tx_index: TxIndex) {
        let mut status = self.lock_tx_status(tx_index);
        assert_eq!(
            *status,
            TransactionStatus::Executing,
            "Only executing transactions can gain status executed. Transaction {tx_index} is not \
             executing. Transaction status: {:?}.",
            *status
        );
        *status = TransactionStatus::Executed;
    }

    fn set_ready_status(&self, tx_index: TxIndex) {
        let mut status = self.lock_tx_status(tx_index);
        assert_eq!(
            *status,
            TransactionStatus::Aborting,
            "Only aborting transactions can be re-executed. Transaction {tx_index} is not \
             aborting. Transaction status: {:?}.",
            *status
        );
        *status = TransactionStatus::ReadyToExecute;
    }

    fn decrease_validation_index(&self, target_index: TxIndex) {
        self.validation_index.fetch_min(target_index, Ordering::SeqCst);
    }

    fn decrease_execution_index(&self, target_index: TxIndex) {
        self.execution_index.fetch_min(target_index, Ordering::SeqCst);
    }

    /// Updates a transaction's status to `Executing` if it is ready to execute.
    fn try_incarnate(&self, tx_index: TxIndex) -> bool {
        let mut status = self.lock_tx_status(tx_index);
        if *status == TransactionStatus::ReadyToExecute {
            *status = TransactionStatus::Executing;
            return true;
        }
        false
    }

    fn next_version_to_validate(&self) -> Option<TxIndex> {
        let index_to_validate = self.validation_index.fetch_add(1, Ordering::SeqCst);

        let status = self.lock_tx_status(index_to_validate);
        if *status == TransactionStatus::Executed {
            return Some(index_to_validate);
        }
        None
    }

    fn next_version_to_execute(&self) -> Option<TxIndex> {
        let index_to_execute = self.execution_index.fetch_add(1, Ordering::SeqCst);
        if self.get_tx_status(index_to_execute) == TransactionStatus::Missing {
            self.decrease_execution_index(index_to_execute);
            return None;
        }
        if self.try_incarnate(index_to_execute) {
            return Some(index_to_execute);
        }
        None
    }

    /// Returns the done marker.
    pub fn done(&self) -> bool {
        self.done_marker.load(Ordering::Acquire)
    }

    /// Sleeps until the scheduler is done or the requested number of committed transactions is
    /// reached.
    pub fn wait_for_completion(&self, target_n_txs: usize) {
        while !(self.done() || self.get_n_committed_txs() >= target_n_txs) {
            std::thread::sleep(std::time::Duration::from_micros(1));
        }

        // Lock and release the commit index to ensure that no commit phase is in progress.
        // Future calls to `try_commit` (which is under the same lock) will exit immediately since
        // the done marker is set.
        drop(self.commit_index.lock());
    }

    #[cfg(any(feature = "testing", test))]
    pub fn set_tx_status(&self, tx_index: TxIndex, status: TransactionStatus) {
        let mut tx_status = self.lock_tx_status(tx_index);
        *tx_status = status;
    }

    /// Returns the status of a transaction without locking it.
    pub fn get_tx_status(&self, tx_index: TxIndex) -> TransactionStatus {
        *self.lock_tx_status(tx_index)
    }
}

#[derive(Debug, PartialEq)]
pub enum Task {
    ExecutionTask(TxIndex),
    ValidationTask(TxIndex),
    AskForTask,
    NoTaskAvailable,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransactionStatus {
    Missing,
    ReadyToExecute,
    Executing,
    Executed,
    Aborting,
    Committed,
}
