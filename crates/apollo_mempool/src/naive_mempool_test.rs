use std::collections::HashMap;

use apollo_mempool_types::mempool_types::{AddTransactionArgs, CommitBlockArgs};
use pretty_assertions::assert_eq;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, nonce, tx_hash};

use crate::naive_mempool::NaiveMempool;
use crate::{add_tx_input, tx};

// Test utilities for NaiveMempool

#[track_caller]
fn add_tx_naive(mempool: &mut NaiveMempool, input: &AddTransactionArgs) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

#[track_caller]
fn commit_block_naive(
    mempool: &mut NaiveMempool,
    nonces: impl IntoIterator<Item = (&'static str, u8)>,
    rejected_tx_hashes: impl IntoIterator<Item = TransactionHash>,
) {
    let nonces = HashMap::from_iter(
        nonces.into_iter().map(|(address, nonce)| (contract_address!(address), nonce!(nonce))),
    );
    let rejected_tx_hashes = rejected_tx_hashes.into_iter().collect();
    let args = CommitBlockArgs { address_to_nonce: nonces, rejected_tx_hashes };

    mempool.commit_block(args);
}

#[track_caller]
fn get_txs_and_assert_expected_naive(
    mempool: &mut NaiveMempool,
    n_txs: usize,
    expected_txs: &[InternalRpcTransaction],
) {
    let txs = mempool.get_txs(n_txs).unwrap();
    assert_eq!(txs, expected_txs);
}

// Tests

#[test]
fn test_add_multiple_txs_fifo_order() {
    let mut mempool = NaiveMempool::new();
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    let tx2 = tx!(tx_hash: 2, address: "0x2", tx_nonce: 0);
    let tx3 = tx!(tx_hash: 3, address: "0x3", tx_nonce: 0);

    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0),
    );

    // Verify FIFO order
    get_txs_and_assert_expected_naive(&mut mempool, 3, &[tx1, tx2, tx3]);
}

#[test]
fn test_get_txs_more_than_available() {
    let mut mempool = NaiveMempool::new();
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    let tx2 = tx!(tx_hash: 2, address: "0x2", tx_nonce: 0);

    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0),
    );

    // Request more than available
    get_txs_and_assert_expected_naive(&mut mempool, 10, &[tx1, tx2]);
}

#[test]
fn test_get_txs_empty() {
    let mut mempool = NaiveMempool::new();
    let empty: Vec<InternalRpcTransaction> = Vec::new();
    get_txs_and_assert_expected_naive(&mut mempool, 5, &empty);
}

#[test]
fn test_commit_block_remove_committed() {
    let mut mempool = NaiveMempool::new();
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    let tx2 = tx!(tx_hash: 2, address: "0x1", tx_nonce: 1);
    let tx3 = tx!(tx_hash: 3, address: "0x1", tx_nonce: 2);

    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0),
    );

    // Get transactions (they're staged)
    get_txs_and_assert_expected_naive(&mut mempool, 3, &[tx1.clone(), tx2.clone(), tx3.clone()]);

    // Commit block: address 0x1 has next_nonce 2 (meaning nonces 0 and 1 were committed)
    commit_block_naive(&mut mempool, [("0x1", 2)], []);

    // Transactions with nonce < 2 (i.e., nonce 0 and 1) should be removed
    // Transaction with nonce 2 should be rewound back to queue
    get_txs_and_assert_expected_naive(&mut mempool, 1, &[tx3]);
}

#[test]
fn test_commit_block_remove_rejected() {
    let mut mempool = NaiveMempool::new();
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    let tx2 = tx!(tx_hash: 2, address: "0x2", tx_nonce: 0);
    let tx3 = tx!(tx_hash: 3, address: "0x3", tx_nonce: 0);

    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0),
    );

    // Get transactions
    get_txs_and_assert_expected_naive(&mut mempool, 3, &[tx1.clone(), tx2.clone(), tx3.clone()]);

    // Commit block: reject tx2
    commit_block_naive(&mut mempool, [], [tx_hash!(2)]);

    // tx2 should be removed, tx1 and tx3 should be rewound
    get_txs_and_assert_expected_naive(&mut mempool, 2, &[tx1, tx3]);
}

#[test]
fn test_commit_block_remove_up_to_nonce() {
    let mut mempool = NaiveMempool::new();
    let tx3 = tx!(tx_hash: 3, address: "0x1", tx_nonce: 2);
    let tx4 = tx!(tx_hash: 4, address: "0x1", tx_nonce: 3);

    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 3, account_nonce: 0),
    );

    // Commit block: address 0x1 has next_nonce 2 (meaning nonces 0 and 1 were committed)
    commit_block_naive(&mut mempool, [("0x1", 2)], []);

    // Transactions with nonce < 2 (i.e., nonce 0 and 1) should be removed
    // Transactions with nonce >= 2 should still be in queue
    get_txs_and_assert_expected_naive(&mut mempool, 2, &[tx3, tx4]);
}

#[test]
fn test_full_add_get_flow() {
    let mut mempool = NaiveMempool::new();

    // Step 1: Add transactions
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    let tx2 = tx!(tx_hash: 2, address: "0x2", tx_nonce: 0);
    let tx3 = tx!(tx_hash: 3, address: "0x3", tx_nonce: 0);

    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x2", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 3, address: "0x3", tx_nonce: 0, account_nonce: 0),
    );

    // Step 2: Get transactions - should return in FIFO order
    let result = mempool.get_txs(3).unwrap();
    assert_eq!(result.len(), 3, "Should return 3 transactions");
    assert_eq!(result[0].tx_hash(), tx1.tx_hash(), "First tx should be tx1");
    assert_eq!(result[1].tx_hash(), tx2.tx_hash(), "Second tx should be tx2");
    assert_eq!(result[2].tx_hash(), tx3.tx_hash(), "Third tx should be tx3");

    // Step 3: Verify queue is drained but pool still has transactions (soft-delete pattern)
    // After get_txs, queue should be empty but transactions should still be in pool
    let empty_result = mempool.get_txs(10).unwrap();
    assert_eq!(empty_result.len(), 0, "Queue should be empty after get_txs");

    // Step 4: Verify transactions are still in pool (they're staged, not deleted)
    // We can verify this by checking that commit_block can still find them
    // If we commit tx1 and tx2, tx3 should be rewound
    commit_block_naive(&mut mempool, [("0x1", 1), ("0x2", 1)], []);

    // Step 5: Verify rewind - tx3 should be back in queue
    let rewound_result = mempool.get_txs(10).unwrap();
    assert_eq!(rewound_result.len(), 1, "Should have 1 rewound transaction");
    assert_eq!(rewound_result[0].tx_hash(), tx3.tx_hash(), "Rewound tx should be tx3");
}

#[test]
fn test_add_get_partial_flow() {
    let mut mempool = NaiveMempool::new();

    // Add 5 transactions
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    let tx2 = tx!(tx_hash: 2, address: "0x2", tx_nonce: 0);
    let tx3 = tx!(tx_hash: 3, address: "0x3", tx_nonce: 0);
    let tx4 = tx!(tx_hash: 4, address: "0x4", tx_nonce: 0);
    let tx5 = tx!(tx_hash: 5, address: "0x5", tx_nonce: 0);

    for i in 1..=5 {
        add_tx_naive(
            &mut mempool,
            &add_tx_input!(tx_hash: i, address: format!("0x{}", i).as_str(), tx_nonce: 0, account_nonce: 0),
        );
    }

    // Get only 2 transactions
    let result = mempool.get_txs(2).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].tx_hash(), tx1.tx_hash());
    assert_eq!(result[1].tx_hash(), tx2.tx_hash());

    // Get remaining 3 transactions
    let result2 = mempool.get_txs(10).unwrap();
    assert_eq!(result2.len(), 3);
    assert_eq!(result2[0].tx_hash(), tx3.tx_hash());
    assert_eq!(result2[1].tx_hash(), tx4.tx_hash());
    assert_eq!(result2[2].tx_hash(), tx5.tx_hash());

    // Queue should now be empty
    let empty = mempool.get_txs(10).unwrap();
    assert_eq!(empty.len(), 0);
}

#[test]
fn test_add_get_commit_rewind_flow() {
    let mut mempool = NaiveMempool::new();

    // Add transactions
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0),
    );
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0),
    );

    // Get all transactions (they're staged)
    let staged = mempool.get_txs(10).unwrap();
    assert_eq!(staged.len(), 3);

    // Commit: tx1 committed (next_nonce=1 means nonce 0 committed), tx3 committed, tx2 rejected
    commit_block_naive(&mut mempool, [("0x1", 1), ("0x2", 1)], [tx_hash!(2)]);

    // tx1 and tx3 are committed, tx2 is rejected - none should be rewound
    let after_commit = mempool.get_txs(10).unwrap();
    assert_eq!(after_commit.len(), 0, "No transactions should be rewound");

    // Add new transaction
    let tx4 = tx!(tx_hash: 4, address: "0x3", tx_nonce: 0);
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 4, address: "0x3", tx_nonce: 0, account_nonce: 0),
    );

    // Should get new transaction
    let new_tx = mempool.get_txs(10).unwrap();
    assert_eq!(new_tx.len(), 1);
    assert_eq!(new_tx[0].tx_hash(), tx4.tx_hash());
}
