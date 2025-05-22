use std::cmp::min;
use std::sync::atomic::Ordering;

use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::concurrency::scheduler::{Scheduler, Task, TransactionStatus};
use crate::concurrency::test_utils::DEFAULT_CHUNK_SIZE;
use crate::concurrency::TxIndex;

#[rstest]
fn test_new() {
    let scheduler = Scheduler::new();
    assert_eq!(scheduler.execution_index.load(Ordering::Acquire), 0);
    assert_eq!(scheduler.validation_index.load(Ordering::Acquire), 0);
    assert_eq!(*scheduler.commit_index.lock().unwrap(), 0);
    assert_eq!(scheduler.tx_statuses.len(), 0);
    for i in 0..10 {
        assert_eq!(scheduler.get_tx_status(i), TransactionStatus::Missing);
    }
    assert_eq!(scheduler.done_marker.into_inner(), false);
}

#[rstest]
fn test_lock_tx_status() {
    let scheduler = Scheduler::new();
    assert_eq!(scheduler.get_tx_status(0), TransactionStatus::Missing);
    scheduler.new_transaction(0);
    let status = scheduler.lock_tx_status(0);
    assert_eq!(*status, TransactionStatus::ReadyToExecute);
}

#[rstest]
#[case::done(DEFAULT_CHUNK_SIZE, DEFAULT_CHUNK_SIZE, TransactionStatus::Executed, Task::Done)]
#[case::no_task(
    DEFAULT_CHUNK_SIZE,
    DEFAULT_CHUNK_SIZE,
    TransactionStatus::Missing,
    Task::NoTaskAvailable
)]
#[case::no_task_as_validation_index_not_executed(
    DEFAULT_CHUNK_SIZE,
    0,
    TransactionStatus::ReadyToExecute,
    Task::AskForTask
)]
#[case::execution_task(0, 0, TransactionStatus::ReadyToExecute, Task::ExecutionTask(0))]
#[case::execution_task_as_validation_index_not_executed(
    1,
    0,
    TransactionStatus::ReadyToExecute,
    Task::ExecutionTask(1)
)]
#[case::validation_task(1, 0, TransactionStatus::Executed, Task::ValidationTask(0))]
fn test_next_task(
    #[case] execution_index: TxIndex,
    #[case] validation_index: TxIndex,
    #[case] validation_index_status: TransactionStatus,
    #[case] expected_next_task: Task,
) {
    let scheduler = Scheduler {
        execution_index: execution_index.into(),
        validation_index: validation_index.into(),
        done_marker: (expected_next_task == Task::Done).into(),
        ..Scheduler::new()
    };
    for i in 0..DEFAULT_CHUNK_SIZE {
        scheduler.new_transaction(i);
    }
    scheduler.set_tx_status(validation_index, validation_index_status);
    let next_task = scheduler.next_task();
    assert_eq!(next_task, expected_next_task);
}

#[rstest]
#[case::happy_flow(0, TransactionStatus::Executed, false)]
#[case::happy_flow_with_halt(0, TransactionStatus::Executed, true)]
#[case::wrong_status_ready(0, TransactionStatus::ReadyToExecute, false)]
#[case::wrong_status_executing(0, TransactionStatus::Executing, false)]
#[case::wrong_status_aborting(0, TransactionStatus::Aborting, false)]
#[case::wrong_status_committed(0, TransactionStatus::Committed, false)]
fn test_commit_flow(
    #[case] commit_index: TxIndex,
    #[case] commit_index_tx_status: TransactionStatus,
    #[case] should_halt: bool,
) {
    let scheduler = Scheduler { commit_index: commit_index.into(), ..Scheduler::new() };
    scheduler.set_tx_status(commit_index, commit_index_tx_status);
    let mut tx_committer = scheduler.try_enter_commit_phase().unwrap();
    // Lock is already acquired.
    assert!(scheduler.try_enter_commit_phase().is_none());
    if let Some(index) = tx_committer.try_commit() {
        assert_eq!(index, commit_index);
    }
    if should_halt {
        tx_committer.uncommit();
        scheduler.halt();
    }
    drop(tx_committer);
    if commit_index_tx_status == TransactionStatus::Executed {
        assert_eq!(*scheduler.lock_tx_status(commit_index), TransactionStatus::Committed);
        assert_eq!(
            *scheduler.commit_index.lock().unwrap(),
            if should_halt { commit_index } else { commit_index + 1 }
        );
    } else {
        assert_eq!(*scheduler.lock_tx_status(commit_index), commit_index_tx_status);
        assert_eq!(*scheduler.commit_index.lock().unwrap(), commit_index);
    }
}

#[rstest]
#[case::happy_flow(TransactionStatus::Executing)]
#[should_panic(expected = "Only executing transactions can gain status executed. Transaction 0 \
                           is not executing. Transaction status: ReadyToExecute.")]
#[case::wrong_status_ready(TransactionStatus::ReadyToExecute)]
#[should_panic(expected = "Only executing transactions can gain status executed. Transaction 0 \
                           is not executing. Transaction status: Executed.")]
#[case::wrong_status_executed(TransactionStatus::Executed)]
#[should_panic(expected = "Only executing transactions can gain status executed. Transaction 0 \
                           is not executing. Transaction status: Aborting.")]
#[case::wrong_status_aborting(TransactionStatus::Aborting)]
#[should_panic(expected = "Only executing transactions can gain status executed. Transaction 0 \
                           is not executing. Transaction status: Committed.")]
#[case::wrong_status_committed(TransactionStatus::Committed)]
fn test_set_executed_status(#[case] tx_status: TransactionStatus) {
    let tx_index = 0;
    let scheduler = Scheduler::new();
    scheduler.set_tx_status(tx_index, tx_status);
    // Panic is expected here in negative flows.
    scheduler.set_executed_status(tx_index);
    assert_eq!(*scheduler.lock_tx_status(tx_index), TransactionStatus::Executed);
}

#[rstest]
#[case::reduces_validation_index(0, 10)]
#[case::does_not_reduce_validation_index(10, 0)]
fn test_finish_execution(#[case] tx_index: TxIndex, #[case] validation_index: TxIndex) {
    let scheduler = Scheduler { validation_index: validation_index.into(), ..Scheduler::new() };
    scheduler.set_tx_status(tx_index, TransactionStatus::Executing);
    scheduler.finish_execution(tx_index);
    assert_eq!(*scheduler.lock_tx_status(tx_index), TransactionStatus::Executed);
    assert_eq!(scheduler.validation_index.load(Ordering::Acquire), min(tx_index, validation_index));
}

#[rstest]
#[case::happy_flow(TransactionStatus::Aborting)]
#[should_panic(expected = "Only aborting transactions can be re-executed. Transaction 0 is not \
                           aborting. Transaction status: ReadyToExecute.")]
#[case::wrong_status_ready(TransactionStatus::ReadyToExecute)]
#[should_panic(expected = "Only aborting transactions can be re-executed. Transaction 0 is not \
                           aborting. Transaction status: Executed.")]
#[case::wrong_status_executed(TransactionStatus::Executed)]
#[should_panic(expected = "Only aborting transactions can be re-executed. Transaction 0 is not \
                           aborting. Transaction status: Executing.")]
#[case::wrong_status_executing(TransactionStatus::Executing)]
#[should_panic(expected = "Only aborting transactions can be re-executed. Transaction 0 is not \
                           aborting. Transaction status: Committed.")]
#[case::wrong_status_committed(TransactionStatus::Committed)]
fn test_set_ready_status(#[case] tx_status: TransactionStatus) {
    let tx_index = 0;
    let scheduler = Scheduler::new();
    scheduler.set_tx_status(tx_index, tx_status);
    // Panic is expected here in negative flows.
    scheduler.set_ready_status(tx_index);
    assert_eq!(*scheduler.lock_tx_status(tx_index), TransactionStatus::ReadyToExecute);
}

#[rstest]
#[case::abort_validation(TransactionStatus::Executed, false)]
#[case::wrong_status_ready(TransactionStatus::ReadyToExecute, false)]
#[case::wrong_status_executing(TransactionStatus::Executing, false)]
#[case::wrong_status_aborted(TransactionStatus::Aborting, false)]
#[case::wrong_status_committed(TransactionStatus::Committed, false)]
#[case::wrong_status_committed(TransactionStatus::Committed, true)]
fn test_try_validation_abort(#[case] tx_status: TransactionStatus, #[case] abort_committed: bool) {
    let tx_index = 0;
    let scheduler = Scheduler::new();
    scheduler.set_tx_status(tx_index, tx_status);
    let result = scheduler.try_validation_abort(tx_index, abort_committed);
    let (expected_result, expected_status) = match tx_status {
        TransactionStatus::Executed => (true, TransactionStatus::Aborting),
        TransactionStatus::Committed if abort_committed => (true, TransactionStatus::Aborting),
        _ => (false, tx_status),
    };
    assert_eq!(result, expected_result);
    assert_eq!(*scheduler.lock_tx_status(tx_index), expected_status);
}

#[rstest]
#[case::same_execution_index(10, 10)]
#[case::larger_execution_index(0, 10)]
#[case::smaller_execution_index(10, 0)]
fn test_finish_abort(#[case] tx_index: TxIndex, #[case] execution_index: TxIndex) {
    let scheduler = Scheduler { execution_index: execution_index.into(), ..Scheduler::new() };
    for i in 0..20 {
        scheduler.new_transaction(i);
    }
    scheduler.set_tx_status(tx_index, TransactionStatus::Aborting);
    scheduler.finish_abort(tx_index);
    assert_eq!(scheduler.get_tx_status(tx_index), TransactionStatus::ReadyToExecute);
    assert_eq!(
        scheduler.execution_index.load(Ordering::Acquire),
        std::cmp::min(tx_index, execution_index)
    );

    if tx_index <= execution_index {
        // Check the next task is re-execution of the transaction.
        assert_eq!(scheduler.next_task(), Task::ExecutionTask(tx_index));
    }
}

#[rstest]
#[case::target_index_lt_validation_index(1, 3)]
#[case::target_index_eq_validation_index(3, 3)]
#[case::target_index_eq_validation_index_eq_zero(0, 0)]
#[case::target_index_gt_validation_index(1, 0)]
fn test_decrease_validation_index(
    #[case] target_index: TxIndex,
    #[case] validation_index: TxIndex,
) {
    let scheduler = Scheduler { validation_index: validation_index.into(), ..Scheduler::new() };
    scheduler.decrease_validation_index(target_index);
    let expected_validation_index = min(target_index, validation_index);
    assert_eq!(scheduler.validation_index.load(Ordering::Acquire), expected_validation_index);
}

#[rstest]
#[case::target_index_lt_execution_index(1, 3)]
#[case::target_index_eq_execution_index(3, 3)]
#[case::target_index_eq_execution_index_eq_zero(0, 0)]
#[case::target_index_gt_execution_index(1, 0)]
fn test_decrease_execution_index(#[case] target_index: TxIndex, #[case] execution_index: TxIndex) {
    let scheduler = Scheduler { execution_index: execution_index.into(), ..Scheduler::new() };
    scheduler.decrease_execution_index(target_index);
    let expected_execution_index = min(target_index, execution_index);
    assert_eq!(scheduler.execution_index.load(Ordering::Acquire), expected_execution_index);
}

#[rstest]
#[case::ready_to_execute(0, TransactionStatus::ReadyToExecute, true)]
#[case::executing(0, TransactionStatus::Executing, false)]
#[case::executed(0, TransactionStatus::Executed, false)]
#[case::aborting(0, TransactionStatus::Aborting, false)]
#[case::committed(0, TransactionStatus::Committed, false)]
fn test_try_incarnate(
    #[case] tx_index: TxIndex,
    #[case] tx_status: TransactionStatus,
    #[case] expected_output: bool,
) {
    let scheduler = Scheduler::new();
    scheduler.set_tx_status(tx_index, tx_status);
    assert_eq!(scheduler.try_incarnate(tx_index), expected_output);
    if expected_output {
        assert_eq!(*scheduler.lock_tx_status(tx_index), TransactionStatus::Executing);
    } else {
        assert_eq!(*scheduler.lock_tx_status(tx_index), tx_status);
    }
}

#[rstest]
#[case::ready_to_execute(1, TransactionStatus::ReadyToExecute, None)]
#[case::executing(1, TransactionStatus::Executing, None)]
#[case::executed(1, TransactionStatus::Executed, Some(1))]
#[case::aborting(1, TransactionStatus::Aborting, None)]
#[case::committed(1, TransactionStatus::Committed, None)]
fn test_next_version_to_validate(
    #[case] validation_index: TxIndex,
    #[case] tx_status: TransactionStatus,
    #[case] expected_output: Option<TxIndex>,
) {
    let scheduler = Scheduler { validation_index: validation_index.into(), ..Scheduler::new() };
    scheduler.set_tx_status(validation_index, tx_status);
    assert_eq!(scheduler.next_version_to_validate(), expected_output);
    let expected_validation_index = validation_index + 1;
    assert_eq!(scheduler.validation_index.load(Ordering::Acquire), expected_validation_index);
}

#[rstest]
#[case::ready_to_execute(1, TransactionStatus::ReadyToExecute, Some(1))]
#[case::executing(1, TransactionStatus::Executing, None)]
#[case::executed(1, TransactionStatus::Executed, None)]
#[case::aborting(1, TransactionStatus::Aborting, None)]
#[case::committed(1, TransactionStatus::Committed, None)]
#[case::committed(1, TransactionStatus::Missing, None)]
fn test_next_version_to_execute(
    #[case] execution_index: TxIndex,
    #[case] tx_status: TransactionStatus,
    #[case] expected_output: Option<TxIndex>,
) {
    let scheduler = Scheduler { execution_index: execution_index.into(), ..Scheduler::new() };
    scheduler.set_tx_status(execution_index, tx_status);
    assert_eq!(scheduler.next_version_to_execute(), expected_output);
    let expected_execution_index =
        if tx_status != TransactionStatus::Missing { execution_index + 1 } else { execution_index };
    assert_eq!(scheduler.execution_index.load(Ordering::Acquire), expected_execution_index);
}
