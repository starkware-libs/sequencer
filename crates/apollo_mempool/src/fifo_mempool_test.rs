use std::collections::HashMap;
use std::sync::Arc;

use apollo_deployment_mode::DeploymentMode;
use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig};
use apollo_time::test_utils::FakeClock;
use rstest::{fixture, rstest};
use starknet_api::test_utils::valid_resource_bounds_for_testing;
use starknet_api::{contract_address, declare_tx_args, tx_hash};
use url::Url;

use crate::add_tx_input;
use crate::mempool::Mempool;
use crate::test_utils::{add_tx, commit_block, declare_add_tx_input, get_txs_and_assert_expected};

// Helper function to create a mempool for echonet mode
fn create_echonet_mempool() -> Mempool {
    let recorder_url: Url = "http://localhost:8080".parse().expect("Valid URL");

    let config = MempoolConfig {
        static_config: MempoolStaticConfig {
            deployment_mode: DeploymentMode::Echonet,
            recorder_url,
            ..Default::default()
        },
        ..Default::default()
    };
    Mempool::new(config, Arc::new(FakeClock::default()))
}

#[fixture]
fn mempool() -> Mempool {
    create_echonet_mempool()
}

// Tests.

#[rstest]
fn test_get_txs_returns_in_fifo_order() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0);
    let input4 = add_tx_input!(tx_hash: 4, address: "0x3", tx_nonce: 0, account_nonce: 0);
    let input5 = add_tx_input!(tx_hash: 5, address: "0x2", tx_nonce: 1, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    timestamps.insert(tx_hash!(4), 1000);
    timestamps.insert(tx_hash!(5), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2, &input3, &input4, &input5] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    // Transactions should be returned in the order they were added.
    get_txs_and_assert_expected(
        &mut mempool,
        5,
        &[input1.tx, input2.tx, input3.tx, input4.tx, input5.tx],
    );
}

#[rstest]
fn test_get_txs_more_than_all_eligible_txs() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    // Request more than available, return only available transactions.
    get_txs_and_assert_expected(&mut mempool, 10, &[input1.tx, input2.tx]);
}

#[rstest]
fn test_get_txs_zero_transactions() {

    let mut mempool = create_echonet_mempool();
    get_txs_and_assert_expected(&mut mempool, 5, &[]);
}

#[rstest]
fn test_get_txs_does_not_return_popped_transactions() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    // Queue is now empty, returning no transactions.
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_commit_block_removes_committed_transactions(mut mempool: Mempool) {
    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    get_txs_and_assert_expected(&mut mempool, 2, &[input1.tx, input2.tx]);

    commit_block(&mut mempool, [("0x1", 2)], []);

    // All transactions are removed.
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_commit_block_rewinds_non_committed_transactions() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx.clone()]);

    // Commit block: transactions with nonces 0,1 are committed.
    commit_block(&mut mempool, [("0x1", 2)], []);

    // Transaction with nonce 2 is rewound back to queue.
    get_txs_and_assert_expected(&mut mempool, 1, &[input3.tx]);
}

#[rstest]
fn test_commit_block_removes_rejected_transactions() {
    let mut mempool = create_echonet_mempool();

    let input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    mempool.update_timestamps(timestamps);

    add_tx(&mut mempool, &input);

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    get_txs_and_assert_expected(&mut mempool, 1, &[input.tx]);

    // Commit block: reject transaction.
    commit_block(&mut mempool, [], [tx_hash!(1)]);

    // Transaction is removed.
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_commit_block_committed_and_rejected_no_rewind() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 11, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 22, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 33, address: "0x1", tx_nonce: 2, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(11), 1000);
    timestamps.insert(tx_hash!(22), 1000);
    timestamps.insert(tx_hash!(33), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

    get_txs_and_assert_expected(&mut mempool, 3, &[input1.tx, input2.tx, input3.tx]);

    // Commit: tx1 is committed, tx2 is rejected, no info about tx3.
    commit_block(&mut mempool, [("0x1", 1)], [tx_hash!(22)]);

    // No transactions are rewound.
    get_txs_and_assert_expected(&mut mempool, 10, &[]);
}

#[rstest]
fn test_commit_block_future_rejected_tx_should_rewind() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let input3 = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    mempool.update_timestamps(timestamps);

    for input in [&input1, &input2, &input3] {
        add_tx(&mut mempool, input);
    }

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

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
fn test_declare_txs_preserve_fifo_order() {
    let mut mempool = create_echonet_mempool();

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

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    timestamps.insert(tx_hash!(3), 1000);
    timestamps.insert(tx_hash!(4), 1000);
    timestamps.insert(tx_hash!(5), 1000);
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

    // Call get_ts() first to set timestamp threshold
    mempool.get_ts();

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
fn test_get_ts_returns_first_tx_timestamp() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // get_ts() should return the timestamp of the first transaction (1000 from pre-populated)
    let timestamp = mempool.get_ts();
    assert_eq!(timestamp, 1000, "get_ts() should return the first tx timestamp");
}

#[rstest]
fn test_get_ts_returns_zero_when_queue_empty(mut mempool: Mempool) {
    // Queue is empty
    let timestamp = mempool.get_ts();
    assert_eq!(timestamp, 0, "get_ts() should return 0 when queue is empty");
}

// #[rstest]
// fn test_get_txs_returns_empty_if_get_ts_not_called() {
//     let (mut mempool, _server) = create_mempool_with_mock_server();
//
//     let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
//
//     add_tx(&mut mempool, &input1);
//
//     // Don't call get_ts() - get_txs() should return empty
//     get_txs_and_assert_expected(&mut mempool, 1, &[]);
// }

#[rstest]
fn test_get_txs_returns_txs_after_get_ts_called() {
    let mut mempool = create_echonet_mempool();

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    // Pre-populate timestamps
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), 1000);
    timestamps.insert(tx_hash!(2), 1000);
    mempool.update_timestamps(timestamps);

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    let returned_timestamp = mempool.get_ts();
    assert_eq!(returned_timestamp, 1000); // Pre-populated timestamp

    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs.len(), 2);
    assert_eq!(txs[0].tx_hash(), input1.tx.tx_hash());
    assert_eq!(txs[1].tx_hash(), input2.tx.tx_hash());
}

#[rstest]
fn test_pop_ready_chunk_filters_by_exact_timestamp_match() {
    let mut mempool = create_echonet_mempool();
    
    let timestamp1 = 1000u64;
    let timestamp2 = 2000u64;

    let input1 = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let input2 = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);

    // Pre-populate timestamps with different values
    let mut timestamps = HashMap::new();
    timestamps.insert(tx_hash!(1), timestamp1);
    timestamps.insert(tx_hash!(2), timestamp2);
    mempool.update_timestamps(timestamps);

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // First get_ts() returns timestamp1
    let returned_timestamp = mempool.get_ts();
    assert_eq!(returned_timestamp, timestamp1);

    // Only transactions with exact timestamp match (timestamp1) should be returned
    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs.len(), 1, "Should return only tx with timestamp1");
    assert_eq!(txs[0].tx_hash(), input1.tx.tx_hash());

    // Second get_ts() returns timestamp2
    let returned_timestamp2 = mempool.get_ts();
    assert_eq!(returned_timestamp2, timestamp2);

    // Now the second transaction should be returned
    let txs2 = mempool.get_txs(1).unwrap();
    assert_eq!(txs2.len(), 1, "Should return tx with timestamp2");
    assert_eq!(txs2[0].tx_hash(), input2.tx.tx_hash());
}
