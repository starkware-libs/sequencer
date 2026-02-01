use std::sync::Arc;

use apollo_mempool_config::config::{MempoolConfig, MempoolStaticConfig, QueueType};
use apollo_time::test_utils::FakeClock;
use rstest::{fixture, rstest};
use starknet_api::tx_hash;

use crate::add_tx_input;
use crate::mempool::Mempool;
use crate::test_utils::{add_tx, commit_block, get_txs_and_assert_expected};

#[fixture]
fn mempool() -> Mempool {
    let config = MempoolConfig {
        static_config: MempoolStaticConfig { queue_type: QueueType::Fifo, ..Default::default() },
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
