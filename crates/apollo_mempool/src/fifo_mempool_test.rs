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

    // Commit both transactions (account nonce updated to 2)
    commit_block(&mut mempool, [("0x1", 2)], []);

    // Both committed txs are removed from mempool (tx_pool and queue).
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_commit_block_rewinds_non_committed_transactions(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1000, 100));

    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx.clone()]);

    // Commit block: transactions with nonces 0,1 are committed.
    commit_block(&mut mempool, [("0x1", 2)], []);

    // Transaction with nonce 2 is rewound back to queue.
    get_txs_and_assert_expected(&mut mempool, 1, &[input3.tx]);
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
    let input1 = add_tx_input!(tx_hash: 11, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 22, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 33, address: "0x1", tx_nonce: 2, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(11), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(22), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(33), tx_metadata(1000, 100));

    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx]);

    // Commit: tx1 is committed, tx2 is rejected, no info about tx3.
    commit_block(&mut mempool, [("0x1", 1)], [tx_hash!(22)]);

    // No transactions are rewound.
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_commit_block_future_rejected_tx_should_rewind(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1000, 100));

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

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // First call returns timestamp of first tx in queue.
    let first_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(first_timestamp, 1000);

    // Consume all txs, emptying the queue.
    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // After queue is empty, resolve_batch_timestamp() should persist and return the last timestamp
    // (1000).
    // This ensures consistency - we don't suddenly return 0 or a different value.
    let second_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(second_timestamp, 1000, "Timestamp should persist after queue is emptied");
}

#[rstest]
fn test_get_txs_does_not_return_txs_with_different_timestamp(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x4", tx_nonce: 0, account_nonce: 0);

    // Pre-populate timestamps: first two txs have timestamp 1000, next two have 2000
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1001, 101));
    mempool.update_tx_block_metadata(tx_hash!(4), tx_metadata(1001, 101));

    // Add all transactions
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);
    add_tx(&mut mempool, &input4);

    // First resolve_batch_timestamp() should return 1000 (first tx timestamp)
    let first_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(first_timestamp, 1000);

    // Request one transaction from the first timestamp batch.
    get_txs_and_assert_expected(&mut mempool, 1, &[input1.tx]);

    // Request more than what's left in the current timestamp batch.
    // get_txs should pause at the timestamp boundary and return only txs with timestamp 1000.
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx]);

    // Without resolving batch timestamp again, get_txs should return empty
    // because the next txs have a different timestamp (1001 != 1000)
    let txs = mempool.get_txs(10).unwrap();
    assert_eq!(txs, &[], "get_txs should return empty when next tx has different timestamp");

    // Now resolve batch timestamp again to update threshold to 1001
    let second_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(second_timestamp, 1001);

    // Now get_txs should return the remaining txs with timestamp 1001
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

    assert_eq!(mempool.resolve_batch_timestamp(), 1000);
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

    assert_eq!(mempool.resolve_batch_timestamp(), 100);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input1.tx, input2.tx]);

    assert_eq!(mempool.resolve_batch_timestamp(), 200);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input3.tx]);

    // Block 4 is missing.
    assert_eq!(mempool.resolve_batch_timestamp(), 300);
    assert_eq!(mempool.get_txs(10).unwrap(), Vec::new());

    // Block 5 is present.
    assert_eq!(mempool.resolve_batch_timestamp(), 300);
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

    assert_eq!(mempool.resolve_batch_timestamp(), 100);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input1.tx]);
    assert_eq!(mempool.resolve_batch_timestamp(), 200);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input2.tx]);

    for _ in 0..19 {
        assert_eq!(mempool.resolve_batch_timestamp(), 300);
        assert_eq!(mempool.get_txs(10).unwrap(), Vec::new());
    }

    assert_eq!(mempool.resolve_batch_timestamp(), 300);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input3.tx]);
}

#[rstest]
fn test_get_txs_after_queue_emptied_still_resolves_new_tx(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(100, 1));
    add_tx(&mut mempool, &input1);

    assert_eq!(mempool.resolve_batch_timestamp(), 100);
    assert_eq!(mempool.get_txs(10).unwrap(), vec![input1.tx]);

    assert_eq!(mempool.resolve_batch_timestamp(), 100);
    assert_eq!(mempool.get_txs(10).unwrap(), Vec::new());

    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(200, 2));
    add_tx(&mut mempool, &input2);
    assert_eq!(mempool.resolve_batch_timestamp(), 200);
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

    assert_eq!(mempool.resolve_batch_timestamp(), 1000);
    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx.clone()]);

    // Only tx 1 and 2 are committed; tx3 rewinds.
    commit_block(&mut mempool, [("0x1", 2)], []);

    assert_eq!(mempool.resolve_batch_timestamp(), 1000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input3.tx, input4.tx]);
    assert_eq!(mempool.resolve_batch_timestamp(), 2000);
    get_txs_and_assert_expected(&mut mempool, 10, &[input5.tx]);
}

#[rstest]
fn test_rewind_preserves_timestamp_order(mut mempool: Mempool) {
    // This test reproduces the bug where rewound transactions go to the back of the queue
    // instead of the front, causing them to be processed after new transactions with
    // different timestamps.

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x3", tx_nonce: 0, account_nonce: 0);

    // Set metadata for all transactions (timestamp controls ordering in these tests).
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1000, 100)); // tx3 also has timestamp 1000
    mempool.update_tx_block_metadata(tx_hash!(4), tx_metadata(1001, 101)); // tx4 has timestamp 1001

    // Add tx1, tx2, tx3 (all with timestamp 1000)
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);

    // Get timestamp and fetch only tx1 and tx2 (leave tx3 in queue)
    let first_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(first_timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx.clone()]);

    // At this point: tx3 is still in the queue
    // Commit block: only tx1 is committed, tx2 should be rewound
    // tx3 is still in the queue
    commit_block(&mut mempool, [("0x1", 1)], []);

    // NOW add tx4 with timestamp 1001
    add_tx(&mut mempool, &input4);

    // resolve_batch_timestamp() should return 1000 (from rewound tx2 at front)
    // NOT 1001 (from new tx4 at back)
    let second_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(
        second_timestamp, 1000,
        "resolve_batch_timestamp() should return timestamp of rewound tx (1000), not new tx (1001)"
    );

    // get_txs should return tx2 and tx3 (both timestamp 1000), not tx4 (timestamp 1001)
    // The order should be tx2 (rewound, should be first), then tx3
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx, input3.tx]);

    // Now resolve_batch_timestamp() should return 1001 for the next batch
    let third_timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(third_timestamp, 1001);

    // And get_txs should return tx4
    get_txs_and_assert_expected(&mut mempool, 1, &[input4.tx]);
}

#[rstest]
fn test_rewind_maintains_fifo_order_with_mixed_results(mut mempool: Mempool) {
    // When a block is committed with mixed results (some committed, some rejected, some unknown),
    // rewound transactions should maintain their original FIFO order.
    // This test verifies: tx1 committed, tx2 unknown (rewound), tx3 rejected (rewound)
    // Result: tx2, tx3 maintain original order (tx2 before tx3)

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x2", tx_nonce: 0, account_nonce: 0);

    // Set metadata: tx1,2,3 -> timestamp 1000, tx4 -> timestamp 2000
    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(3), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(4), tx_metadata(1001, 101));

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);
    add_tx(&mut mempool, &input4);

    let ts1 = mempool.resolve_batch_timestamp();
    assert_eq!(ts1, 1000);

    // Retrieve first batch (timestamp 1000)
    get_txs_and_assert_expected(
        &mut mempool,
        5,
        &[input1.tx, input2.tx.clone(), input3.tx.clone()],
    );

    // Commit block: tx1 committed, tx2 not mentioned (rewound), tx3 rejected (also rewound)
    commit_block(&mut mempool, [("0x1", 1)], [tx_hash!(3)]);

    // Rewound txs (tx2, tx3) should maintain original order and have timestamp 1000
    let ts2 = mempool.resolve_batch_timestamp();
    assert_eq!(ts2, 1000);
    get_txs_and_assert_expected(&mut mempool, 3, &[input2.tx, input3.tx]);

    // Next batch has timestamp 1001
    let ts3 = mempool.resolve_batch_timestamp();
    assert_eq!(ts3, 1001);
    get_txs_and_assert_expected(&mut mempool, 1, &[input4.tx]);
}

#[rstest]
fn test_resolve_batch_timestamp_returns_zero_when_never_had_transactions(mut mempool: Mempool) {
    // When mempool has never had any transactions, resolve_batch_timestamp() should return 0.
    let timestamp = mempool.resolve_batch_timestamp();
    assert_eq!(
        timestamp, 0,
        "resolve_batch_timestamp() should return 0 when queue has never had txs"
    );
}

#[rstest]
fn test_rewind_many_transactions_from_same_address(mut mempool: Mempool) {
    // Add 5 transactions from the same address with nonces 0-4
    let input0 = add_tx_input!(tx_hash: 10, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input1 = add_tx_input!(tx_hash: 11, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 12, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 13, address: "0x1", tx_nonce: 3, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 14, address: "0x1", tx_nonce: 4, account_nonce: 0);

    // Set timestamps for all transactions
    for i in 0..=4 {
        mempool.update_tx_block_metadata(tx_hash!(10 + i), tx_metadata(1000, 100));
    }

    // Add all 5 transactions
    add_tx(&mut mempool, &input0);
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);
    add_tx(&mut mempool, &input4);

    // Keep references to txs that are asserted again after commit.
    let tx2 = input2.tx.clone();
    let tx3 = input3.tx.clone();
    let tx4 = input4.tx.clone();

    // Get all 5 transactions (staging them)
    let ts = mempool.resolve_batch_timestamp();
    assert_eq!(ts, 1000);
    get_txs_and_assert_expected(
        &mut mempool,
        10,
        &[input0.tx, input1.tx, tx2.clone(), tx3.clone(), tx4.clone()],
    );

    // Commit only the first 2 transactions (nonces 0 and 1)
    // The remaining 3 transactions (nonces 2, 3, 4) should be rewound
    commit_block(&mut mempool, [("0x1", 2)], []);

    // Verify that the 3 non-committed transactions are rewound and can be retrieved again
    let ts2 = mempool.resolve_batch_timestamp();
    assert_eq!(ts2, 1000, "Rewound transactions should keep their original timestamp");
    get_txs_and_assert_expected(&mut mempool, 10, &[tx2, tx3, tx4]);

    // Commit the remaining transactions to verify they're all valid
    commit_block(&mut mempool, [("0x1", 5)], []);

    // Queue should now be empty
    let ts3 = mempool.resolve_batch_timestamp();
    assert_eq!(ts3, 1000, "No new timestamp since no new transactions");
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

    mempool.update_tx_block_metadata(tx_hash!(1), tx_metadata(1000, 100));
    mempool.update_tx_block_metadata(tx_hash!(2), tx_metadata(1000, 100));
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // Let both queued txs expire before they are fetched.
    fake_clock.advance(Duration::from_secs(65));

    // Both txs are popped then pruned as expired, so no tx should be returned.
    assert_eq!(mempool.resolve_batch_timestamp(), 1000);
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
    assert_eq!(mempool.resolve_batch_timestamp(), 1000);
    let rejected_tx_hash = rejected_tx.tx.tx_hash;
    let expected_rejected = vec![rejected_tx.tx];
    get_txs_and_assert_expected(&mut mempool, 10, &expected_rejected);

    // Reject tx hash 1. In FIFO, this should remove same-address queued txs (tx hash 2).
    commit_block(&mut mempool, [], [rejected_tx_hash]);

    // Next timestamp batch should include only the other address tx.
    assert_eq!(mempool.resolve_batch_timestamp(), 1001);
    let expected_other = vec![other_address_tx.tx];
    get_txs_and_assert_expected(&mut mempool, 10, &expected_other);
}
