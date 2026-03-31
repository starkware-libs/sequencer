use std::sync::Arc;
use std::time::Duration;

use apollo_config::behavior_mode::BehaviorMode;
use apollo_mempool_config::config::{MempoolConfig, MempoolDynamicConfig, MempoolStaticConfig};
use apollo_time::test_utils::FakeClock;
use rstest::{fixture, rstest};
use starknet_api::test_utils::valid_resource_bounds_for_testing;
use starknet_api::{contract_address, declare_tx_args, tx_hash};

use crate::add_tx_input;
use crate::mempool::Mempool;
use crate::test_utils::{
    add_tx,
    commit_block,
    declare_add_tx_input,
    get_txs_and_assert_expected,
    tx_metadata,
};

#[fixture]
fn mempool() -> Mempool {
    let config = MempoolConfig {
        static_config: MempoolStaticConfig {
            behavior_mode: BehaviorMode::Echonet,
            ..Default::default()
        },
        ..Default::default()
    };
    Mempool::new(config, Arc::new(FakeClock::default()))
}

// Tests.

#[rstest]
fn test_get_txs_returns_in_fifo_order(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0, tip: 500);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0, tip: 1);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0, tip: 300);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x3", tx_nonce: 0, account_nonce: 0, tip: 999);
    let input5 = add_tx_input!(tx_hash: 5, address: "0x2", tx_nonce: 1, account_nonce: 0, tip: 2);

    // Set tx block metadata for all transactions.
    for i in 1..=5 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }

    for input in [&input1, &input2, &input3, &input4, &input5] {
        add_tx(&mut mempool, input);
    }

    // Transactions should be returned in insertion order, regardless of tip.
    get_txs_and_assert_expected(
        &mut mempool,
        5,
        &[input1.tx, input2.tx, input3.tx, input4.tx, input5.tx],
    );
}

#[rstest]
fn test_get_txs_more_than_all_eligible_txs(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Request more than available, return only available transactions.
    get_txs_and_assert_expected(&mut mempool, 10, &[input1.tx, input2.tx]);
}

#[rstest]
fn test_get_txs_zero_transactions(mut mempool: Mempool) {
    get_txs_and_assert_expected(&mut mempool, 5, &[]);
}

#[rstest]
fn test_get_txs_consumes_transactions_from_queue(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // Queue is now empty, returning no transactions.
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_committed_txs_removed_from_mempool(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // Commit both transactions (account nonce updated to 2).
    commit_block(&mut mempool, [("0x1", 2)], []);

    // Both committed txs are removed from mempool (tx_pool and queue).
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_commit_block_removes_rejected_transactions(mut mempool: Mempool) {
    let input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));

    add_tx(&mut mempool, &input);

    get_txs_and_assert_expected(&mut mempool, 1, &[input.tx]);

    // Commit block: reject transaction.
    commit_block(&mut mempool, [], [tx_hash!(1)]);

    // Transaction is removed.
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_commit_block_committed_and_rejected_no_rewind(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    for i in 1..=3 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }
    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx]);

    // Commit: tx1 is committed, tx2 is rejected, no info about tx3.
    commit_block(&mut mempool, [("0x1", 1)], [tx_hash!(2)]);

    // No transactions are rewound.
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_commit_block_future_rejected_tx_should_rewind(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    for i in 1..=3 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }
    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(
        &mut mempool,
        3,
        &[input1.tx, input2.tx.clone(), input3.tx.clone()],
    );

    // Commit: tx1 is committed, no info about tx2, tx3 is rejected.
    commit_block(&mut mempool, [("0x1", 1)], [tx_hash!(3)]);

    // tx2 and tx3 are rewound.
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx, input3.tx]);
}

#[rstest]
fn test_declare_txs_preserve_fifo_order(mut mempool: Mempool) {
    let tx1_declare_account1_input = declare_add_tx_input(
        declare_tx_args!(resource_bounds: valid_resource_bounds_for_testing(), sender_address: contract_address!("0x1"), tx_hash: tx_hash!(1)),
    );
    let tx2_invoke_account1_input =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx3_invoke_account2_input =
        add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let tx4_declare_account4_input = declare_add_tx_input(
        declare_tx_args!(resource_bounds: valid_resource_bounds_for_testing(), sender_address: contract_address!("0x4"), tx_hash: tx_hash!(4)),
    );
    let tx5_invoke_account5_input =
        add_tx_input!(tx_hash: 5, address: "0x5", tx_nonce: 0, account_nonce: 0);

    for i in 1..=5 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }

    for input in [
        &tx1_declare_account1_input,
        &tx2_invoke_account1_input,
        &tx3_invoke_account2_input,
        &tx4_declare_account4_input,
        &tx5_invoke_account5_input,
    ] {
        add_tx(&mut mempool, input);
    }

    // All transactions should be returned in the exact order they were added (FIFO).
    // Declares are NOT delayed, they maintain FIFO order.
    get_txs_and_assert_expected(
        &mut mempool,
        5,
        &[
            tx1_declare_account1_input.tx,
            tx2_invoke_account1_input.tx,
            tx3_invoke_account2_input.tx,
            tx4_declare_account4_input.tx,
            tx5_invoke_account5_input.tx,
        ],
    );
}

#[rstest]
fn test_resolve_batch_timestamp_persists_after_queue_emptied(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    for i in 1..=2 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }
    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);

    // Consume all txs, emptying the queue.
    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // Timestamp should persist after queue is emptied.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
}

#[rstest]
fn test_get_txs_does_not_return_txs_with_different_timestamp(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x4", tx_nonce: 0, account_nonce: 0);

    // txs 1,2 have timestamp 1000; txs 3,4 have timestamp 1001.
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1001, 101));
    mempool.update_tx_block_metadata(tx_hash!(4), tx_metadata(1001, 101));

    for input in [&input1, &input2, &input3, &input4] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);

    // Request one transaction from the first timestamp batch.
    get_txs_and_assert_expected(&mut mempool, 1, &[input1.tx]);

    // get_txs pauses at the timestamp boundary; only returns remaining txs with timestamp 1000.
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx]);

    // Without resolving block metadata, get_txs returns empty (next txs have timestamp 1001).
    assert_eq!(mempool.get_txs(10).unwrap(), vec![]);

    // Resolve to advance to timestamp 1001.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1001);
    get_txs_and_assert_expected(&mut mempool, 10, &[input3.tx, input4.tx]);
}

#[rstest]
fn test_get_txs_same_block_spans_multiple_chunks(mut mempool: Mempool) {
    for i in 1..=111 {
        let addr = format!("0x{}", i);
        let input =
            add_tx_input!(tx_hash: i, address: addr.as_str(), tx_nonce: 0, account_nonce: 0);
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 10));
        add_tx(&mut mempool, &input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    // First chunk: block builder fetches 100 txs.
    let chunk1 = mempool.get_txs(100).unwrap();
    assert_eq!(chunk1.len(), 100);
    // Second chunk: block builder has capacity again, fetches remaining 11.
    let chunk2 = mempool.get_txs(100).unwrap();
    assert_eq!(chunk2.len(), 11);
    assert_eq!(chunk1.len() + chunk2.len(), 111);
}

#[rstest]
fn test_get_txs_pauses_once_on_block_number_gap(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x4", tx_nonce: 0, account_nonce: 0);
    let input5 = add_tx_input!(tx_hash: 5, address: "0x5", tx_nonce: 0, account_nonce: 0);

    // Blocks 2, 3, 5 — gap at block 4: one empty block, then block 5.
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(100, 2));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(100, 2));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(200, 3));
    mempool.update_tx_block_metadata(tx_hash!(4), tx_metadata(300, 5));
    mempool.update_tx_block_metadata(tx_hash!(5), tx_metadata(300, 5));

    for input in [&input1, &input2, &input3, &input4, &input5] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 100);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input1.tx, input2.tx]);

    assert_eq!(mempool.resolve_block_metadata().timestamp, 200);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input3.tx]);

    // Block 4 is missing.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 300);
    assert_eq!(mempool.get_txs(10).unwrap(), Vec::new());

    // Block 5 is present.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 300);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input4.tx, input5.tx]);
}

#[rstest]
fn test_get_txs_returns_empty_result_with_gaps(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0);

    // Blocks 10, 11, 31 — missing 12..=30 (19 empty blocks).
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(100, 10));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(200, 11));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(300, 31));

    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 100);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input1.tx]);
    assert_eq!(mempool.resolve_block_metadata().timestamp, 200);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input2.tx]);

    for _ in 0..19 {
        assert_eq!(mempool.resolve_block_metadata().timestamp, 300);
        assert_eq!(mempool.get_txs(10).unwrap(), Vec::new());
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 300);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input3.tx]);
}

#[rstest]
fn test_get_txs_after_queue_emptied_still_resolves_new_tx(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(100, 1));
    add_tx(&mut mempool, &input1);

    assert_eq!(mempool.resolve_block_metadata().timestamp, 100);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input1.tx]);

    assert_eq!(mempool.resolve_block_metadata().timestamp, 100);
    assert_eq!(mempool.get_txs(10).unwrap(), Vec::new());

    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(200, 2));
    add_tx(&mut mempool, &input2);
    assert_eq!(mempool.resolve_block_metadata().timestamp, 200);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input2.tx]);
}

#[rstest]
fn test_rewind_partial_block_then_continue_to_next_block(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input5 = add_tx_input!(tx_hash: 5, address: "0x3", tx_nonce: 0, account_nonce: 0);
    for h in 1..=4 {
        mempool.update_tx_block_metadata(tx_hash!(h), tx_metadata(1000, 1));
    }
    mempool.update_tx_block_metadata(tx_hash!(5), tx_metadata(2000, 2));
    for input in [&input1, &input2, &input3, &input4, &input5] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx.clone()]);

    // Only tx 1 and 2 are committed; tx3 rewinds.
    commit_block(&mut mempool, [("0x1", 2)], []);

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input3.tx, input4.tx]);
    assert_eq!(mempool.resolve_block_metadata().timestamp, 2000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input5.tx]);
}

#[rstest]
fn test_realign_to_earlier_block_after_rewind(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 1));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(2000, 2));

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // Drain block 1. Expected_block_number = 2.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, std::slice::from_ref(&input1.tx));

    // Rewind tx of block 1. Expected_block_number is still 2.
    commit_block(&mut mempool, [], []);

    // Realign to block 1.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input1.tx]);

    // Now expected_block_number has advanced to 2 again; block-2 tx is next.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 2000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx]);
}

#[rstest]
fn test_rewind_preserves_timestamp_order(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x3", tx_nonce: 0, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(4), tx_metadata(1001, 101));

    // Add tx1, tx2, tx3 (timestamp 1000); tx4 (timestamp 1001) is added after the rewind.
    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    // Fetch tx1 and tx2; leave tx3 in queue.
    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx.clone()]);

    // Commit only tx1; tx2 is rewound to the front.
    commit_block(&mut mempool, [("0x1", 1)], []);

    // Add tx4 (timestamp 1001) after the rewind.
    add_tx(&mut mempool, &input4);

    // Rewound tx2 is at the front → timestamp must still be 1000, not 1001.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx, input3.tx]);

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1001);
    get_txs_and_assert_expected(&mut mempool, 1, &[input4.tx]);
}

#[rstest]
fn test_resolve_batch_timestamp_returns_zero_when_never_had_transactions(mut mempool: Mempool) {
    assert_eq!(mempool.resolve_block_metadata().timestamp, 0);
}

#[rstest]
fn test_rewind_many_transactions_from_same_address(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 3, account_nonce: 0);
    let input5 = add_tx_input!(tx_hash: 5, address: "0x1", tx_nonce: 4, account_nonce: 0);

    for i in 1..=5 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }
    for input in [&input1, &input2, &input3, &input4, &input5] {
        add_tx(&mut mempool, input);
    }

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(
        &mut mempool,
        10,
        &[input1.tx, input2.tx, input3.tx.clone(), input4.tx.clone(), input5.tx.clone()],
    );

    // Commit only nonces 0 and 1; nonces 2, 3, 4 are rewound.
    commit_block(&mut mempool, [("0x1", 2)], []);

    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input3.tx, input4.tx, input5.tx]);

    commit_block(&mut mempool, [("0x1", 5)], []);
    // Queue should now be empty
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_expired_popped_txs_are_not_rewound() {
    let fake_clock = Arc::new(FakeClock::default());
    let mut mempool = Mempool::new(
        MempoolConfig {
            static_config: MempoolStaticConfig {
                behavior_mode: BehaviorMode::Echonet,
                ..Default::default()
            },
            dynamic_config: MempoolDynamicConfig { transaction_ttl: Duration::from_secs(60) },
        },
        fake_clock.clone(),
    );

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);

    for i in 1..=2 {
        mempool.update_tx_block_metadata(tx_hash!(i), tx_metadata(1000, 100));
    }
    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Let both queued txs expire before they are fetched.
    fake_clock.advance(Duration::from_secs(65));

    // Both txs are popped then pruned as expired, so no tx should be returned.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![]);

    // Commit should not rewind expired popped txs back into queue.
    commit_block(&mut mempool, [], []);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![]);
}

#[rstest]
fn test_rejected_tx_removes_same_address_from_fifo_queue(mut mempool: Mempool) {
    let rejected_tx = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let future_same_address_tx =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let other_address_tx = add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1001, 101));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1001, 101));

    for input in [&rejected_tx, &future_same_address_tx, &other_address_tx] {
        add_tx(&mut mempool, input);
    }

    // Stage only the first tx (timestamp 1000).
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[rejected_tx.tx]);

    // Reject tx. In FIFO, this removes same-address queued txs.
    commit_block(&mut mempool, [], [tx_hash!(1)]);

    // Next timestamp batch should include only the other address tx.
    assert_eq!(mempool.resolve_block_metadata().timestamp, 1001);
    get_txs_and_assert_expected(&mut mempool, 10, &[other_address_tx.tx]);
}
