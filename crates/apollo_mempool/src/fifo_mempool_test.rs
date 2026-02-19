use std::collections::HashMap;
use std::sync::Arc;

use apollo_deployment_mode::DeploymentMode;
use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig};
use apollo_time::test_utils::FakeClock;
use rstest::{fixture, rstest};
use starknet_api::test_utils::valid_resource_bounds_for_testing;
use starknet_api::{contract_address, declare_tx_args, tx_hash};

use crate::add_tx_input;
use crate::mempool::Mempool;
use crate::test_utils::{add_tx, commit_block, declare_add_tx_input, get_txs_and_assert_expected};

#[fixture]
fn mempool() -> Mempool {
    let config = MempoolConfig {
        static_config: MempoolStaticConfig {
            deployment_mode: DeploymentMode::Echonet,
            ..Default::default()
        },
        ..Default::default()
    };
    Mempool::new(config, Arc::new(FakeClock::default()))
}

// Tests.

#[rstest]
fn test_get_txs_returns_in_fifo_order(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x3", tx_nonce: 0, account_nonce: 0);
    let input5 = add_tx_input!(tx_hash: 5, address: "0x2", tx_nonce: 1, account_nonce: 0);

    // Set timestamps for all transactions
    let mut timestamps = HashMap::new();
    for i in 1..=5 {
        timestamps.insert(tx_hash!(i), 1000);
    }
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2, &input3, &input4, &input5] {
        add_tx(&mut mempool, input);
    }

    // Transactions should be returned in the order they were added.
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

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

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
fn test_get_txs_does_not_return_popped_transactions(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // Queue is now empty, returning no transactions.
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_commit_block_removes_committed_transactions(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    commit_block(&mut mempool, [("0x1", 2)], []);

    // All transactions are removed.
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_commit_block_rewinds_non_committed_transactions(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    mempool.update_timestamps(timestamps);

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

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    mempool.update_timestamps(timestamps);

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

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(11), 1000);
    timestamps.insert(tx_hash!(22), 1000);
    timestamps.insert(tx_hash!(33), 1000);
    mempool.update_timestamps(timestamps);

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

    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    mempool.update_timestamps(timestamps);

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

    let mut timestamps = HashMap::new();
    for i in 1..=5 {
        timestamps.insert(tx_hash!(i), 1000);
    }
    mempool.update_timestamps(timestamps);

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
fn test_get_timestamp_returns_last_returned_when_queue_empty_after_prior_get_ts(
    mut mempool: Mempool,
) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // First call sets and returns timestamp threshold.
    let first_timestamp = mempool.get_timestamp();
    assert_eq!(first_timestamp, 1000, "get_timestamp() should return the first tx timestamp");

    // Drain the queue.
    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // Queue is now empty; get_ts should return the last returned timestamp.
    let second_timestamp = mempool.get_timestamp();
    assert_eq!(
        second_timestamp, 1000,
        "get_timestamp() should return last returned timestamp when queue is empty"
    );
}

#[rstest]
fn test_get_txs_does_not_return_txs_with_different_timestamp(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x4", tx_nonce: 0, account_nonce: 0);

    // Pre-populate timestamps: first two txs have timestamp 1000, next two have 2000
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 2000);
    timestamps.insert(tx_hash!(4), 2000);
    mempool.update_timestamps(timestamps);

    // Add all transactions
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);
    add_tx(&mut mempool, &input4);

    // First get_timestamp() should return 1000 (first tx timestamp)
    let first_timestamp = mempool.get_timestamp();
    assert_eq!(first_timestamp, 1000);

    // First get_txs should only return txs with timestamp 1000
    get_txs_and_assert_expected(&mut mempool, 10, &[input1.tx, input2.tx]);

    // Without calling get_timestamp() again, get_txs should return empty
    // because the next txs have a different timestamp (2000 != 1000)
    let txs = mempool.get_txs(10).unwrap();
    assert_eq!(txs, &[], "get_txs should return empty when next tx has different timestamp");

    // Now call get_timestamp() again to update threshold to 2000
    let second_timestamp = mempool.get_timestamp();
    assert_eq!(second_timestamp, 2000);

    // Now get_txs should return the remaining txs with timestamp 2000
    get_txs_and_assert_expected(&mut mempool, 10, &[input3.tx, input4.tx]);
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

    // Set timestamps for all transactions
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000); // tx3 also has timestamp 1000
    timestamps.insert(tx_hash!(4), 2000); // tx4 has timestamp 2000
    mempool.update_timestamps(timestamps);

    // Add tx1, tx2, tx3 (all with timestamp 1000)
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);

    // Get timestamp and fetch only tx1 and tx2 (leave tx3 in queue)
    let first_timestamp = mempool.get_timestamp();
    assert_eq!(first_timestamp, 1000);
    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx.clone(), input2.tx.clone()]);

    // At this point: tx3 is still in the queue
    // Commit block: only tx1 is committed, tx2 should be rewound
    // tx3 is still in the queue
    commit_block(&mut mempool, [("0x1", 1)], []);

    // NOW add tx4 with timestamp 2000
    add_tx(&mut mempool, &input4);

    // get_timestamp() should return 1000 (from rewound tx2 at front)
    // NOT 2000 (from new tx4 at back)
    let second_timestamp = mempool.get_timestamp();
    assert_eq!(
        second_timestamp, 1000,
        "get_timestamp() should return timestamp of rewound tx (1000), not new tx (2000)"
    );

    // get_txs should return tx2 and tx3 (both timestamp 1000), not tx4 (timestamp 2000)
    // The order should be tx2 (rewound, should be first), then tx3
    get_txs_and_assert_expected(&mut mempool, 10, &[input2.tx, input3.tx]);

    // Now get_timestamp() should return 2000 for the next batch
    let third_timestamp = mempool.get_timestamp();
    assert_eq!(third_timestamp, 2000);

    // And get_txs should return tx4
    get_txs_and_assert_expected(&mut mempool, 1, &[input4.tx]);
}

#[rstest]
fn test_rewind_with_rejected_tx_preserves_order(mut mempool: Mempool) {
    // Test the exact scenario: tx1->1000, tx2->1000, tx3->1000, tx4->2000
    // get_ts -> 1000, get_txs(5) -> tx1,2,3
    // commit: tx1 committed, tx2 not known, tx3 rejected -> tx2,3 rewound
    // get_ts -> 1000, get_txs(3) -> tx2,3
    // get_ts -> 2000, get_txs(1) -> tx4

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x2", tx_nonce: 0, account_nonce: 0);

    // Set timestamps: tx1,2,3 -> 1000, tx4 -> 2000
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    timestamps.insert(tx_hash!(4), 2000);
    mempool.update_timestamps(timestamps);

    // Add all transactions
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);
    add_tx(&mut mempool, &input3);
    add_tx(&mut mempool, &input4);

    // get_timestamp() -> 1000
    let ts1 = mempool.get_timestamp();
    assert_eq!(ts1, 1000);

    // get_txs(5) -> get tx1, tx2, tx3 (all have timestamp 1000)
    get_txs_and_assert_expected(
        &mut mempool,
        5,
        &[input1.tx, input2.tx.clone(), input3.tx.clone()],
    );

    // Commit block: tx1 committed, tx2 not known (not committed/rejected), tx3 rejected
    // Both tx2 and tx3 should be rewound
    commit_block(&mut mempool, [("0x1", 1)], [tx_hash!(3)]);

    // get_timestamp() -> 1000 (from rewound tx2 and tx3)
    let ts2 = mempool.get_timestamp();
    assert_eq!(ts2, 1000, "Should return timestamp of rewound txs");

    // get_txs(3) -> get tx2, tx3 (both rewound, both have timestamp 1000)
    get_txs_and_assert_expected(&mut mempool, 3, &[input2.tx, input3.tx]);

    // get_timestamp() -> 2000 (from tx4)
    let ts3 = mempool.get_timestamp();
    assert_eq!(ts3, 2000, "Should return timestamp of tx4");

    // get_txs(1) -> get tx4
    get_txs_and_assert_expected(&mut mempool, 1, &[input4.tx]);
}
