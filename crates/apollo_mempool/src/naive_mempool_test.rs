use std::collections::HashMap;
use std::sync::Arc;

use apollo_mempool_config::config::MempoolConfig;
use apollo_mempool_types::mempool_types::{AddTransactionArgs, CommitBlockArgs};
use apollo_time::time::DefaultClock;
use pretty_assertions::assert_eq;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, nonce, tx_hash};

use crate::mempool::Mempool;
use crate::{add_tx_input, tx};

// Test utilities for Mempool

#[track_caller]
fn add_tx_naive(mempool: &mut Mempool, input: &AddTransactionArgs) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

#[track_caller]
fn commit_block_naive(
    mempool: &mut Mempool,
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
    mempool: &mut Mempool,
    n_txs: usize,
    expected_txs: &[InternalRpcTransaction],
) {
    let txs = mempool.get_txs(n_txs).unwrap();
    assert_eq!(txs, expected_txs);
}

fn create_mempool() -> Mempool {
    Mempool::new(MempoolConfig::default(), Arc::new(DefaultClock))
}

// Tests

#[test]
fn test_add_multiple_txs_fifo_order() {
    let mut mempool = create_mempool();
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
    let mut mempool = create_mempool();
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
    let mut mempool = create_mempool();
    let empty: Vec<InternalRpcTransaction> = Vec::new();
    get_txs_and_assert_expected_naive(&mut mempool, 5, &empty);
}

#[test]
fn test_commit_block_remove_committed() {
    let mut mempool = create_mempool();
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
    let mut mempool = create_mempool();
    let tx1_input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx2_input = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx3_input = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);

    let tx1 = tx1_input.tx.clone();
    let tx2 = tx2_input.tx.clone();
    let tx3 = tx3_input.tx.clone();

    add_tx_naive(&mut mempool, &tx1_input);
    add_tx_naive(&mut mempool, &tx2_input);
    add_tx_naive(&mut mempool, &tx3_input);

    // Get transactions (they're staged)
    get_txs_and_assert_expected_naive(&mut mempool, 3, &[tx1.clone(), tx2.clone(), tx3.clone()]);

    // Commit block: tx1 is not committed (address not in committed list), tx2 is rejected, tx3
    // is rejected
    // Expected: All three should be rewound because the first tx (tx1) is not committed
    commit_block_naive(&mut mempool, [], [tx_hash!(2), tx_hash!(3)]);

    // All transactions should be rewound (tx1 is not committed, so all txs from this account are
    // rewound)
    get_txs_and_assert_expected_naive(&mut mempool, 3, &[tx1, tx2, tx3]);
}

#[test]
fn test_commit_block_remove_up_to_nonce() {
    let mut mempool = create_mempool();
    let tx1_input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx2_input = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx3_input = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let tx4_input = add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 3, account_nonce: 0);

    let tx1 = tx1_input.tx.clone();
    let tx2 = tx2_input.tx.clone();
    let tx3 = tx3_input.tx.clone();
    let tx4 = tx4_input.tx.clone();

    // Step 1: Add transactions
    add_tx_naive(&mut mempool, &tx1_input);
    add_tx_naive(&mut mempool, &tx2_input);
    add_tx_naive(&mut mempool, &tx3_input);
    add_tx_naive(&mut mempool, &tx4_input);

    // Step 2: Get transactions (they're staged, removed from queue)
    get_txs_and_assert_expected_naive(
        &mut mempool,
        4,
        &[tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()],
    );

    // Step 3: Commit block: address 0x1 has next_nonce 2 (meaning nonces 0 and 1 were committed)
    // tx1 and tx2 are committed and removed from pool
    // tx3 and tx4 are not committed, so they should be rewound
    commit_block_naive(&mut mempool, [("0x1", 2)], []);

    // Step 4: Get transactions - tx3 and tx4 should be rewound back to queue
    get_txs_and_assert_expected_naive(&mut mempool, 2, &[tx3, tx4]);
}

#[test]
fn test_full_add_get_flow() {
    let mut mempool = create_mempool();

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
    let mut mempool = create_mempool();

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
    let mut mempool = create_mempool();

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

#[test]
fn test_rewind_staged_tx_with_rejected_following_tx() {
    // Test the problematic flow:
    // 1. Add tx with nonce x (should be rewound - was staged but not committed)
    // 2. Add tx with nonce x+1, get it (it's now staged)
    // 3. tx2 is rejected in commit_block
    // 4. Both should be back in queue after commit_block
    let mut mempool = create_mempool();

    // Add tx1 with nonce 0
    let tx1 = tx!(tx_hash: 1, address: "0x1", tx_nonce: 0);
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0),
    );

    // Add tx2 with nonce 1
    let tx2 = tx!(tx_hash: 2, address: "0x1", tx_nonce: 1);
    add_tx_naive(
        &mut mempool,
        &add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0),
    );

    // Get both transactions (they're now staged)
    let staged = mempool.get_txs(10).unwrap();
    assert_eq!(staged.len(), 2);
    assert_eq!(staged[0].tx_hash(), tx1.tx_hash());
    assert_eq!(staged[1].tx_hash(), tx2.tx_hash());

    // Commit: tx1 was staged but NOT committed (address not in committed list)
    // tx2 is rejected
    // Expected: Both tx1 and tx2 should be rewound back to queue
    // - tx1 is rewound because address is not committed
    // - tx2 is rewound because address is in addresses_to_rewind (from tx1)
    commit_block_naive(&mut mempool, [], [tx_hash!(2)]);

    // Both transactions should be back in queue
    let rewound = mempool.get_txs(10).unwrap();
    assert_eq!(
        rewound.len(),
        2,
        "Both tx1 (staged, not committed) and tx2 (rejected) should be rewound"
    );

    // Check order: Since we rewind in reverse order and push_front:
    // - staged_txs: [tx1, tx2] (FIFO order)
    // - We iterate in reverse: [tx2, tx1]
    // - We push_front tx2 first: queue = [tx2]
    // - We push_front tx1: queue = [tx1, tx2]
    // So final order should be [tx1, tx2]
    assert_eq!(rewound[0].tx_hash(), tx1.tx_hash(), "tx1 should be first (rewound from staged)");
    assert_eq!(
        rewound[1].tx_hash(),
        tx2.tx_hash(),
        "tx2 should be second (rejected but rewound because address needs rewind)"
    );
}

#[test]
fn test_commit_block_remove_up_to_nonce_2() {
    // Test 1: commit_block up to nonce 2 - delete tx 0-1, leave tx 2-3
    let mut mempool = create_mempool();
    let tx0_input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx1_input = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx2_input = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let tx3_input = add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 3, account_nonce: 0);

    let tx0 = tx0_input.tx.clone();
    let tx1 = tx1_input.tx.clone();
    let tx2 = tx2_input.tx.clone();
    let tx3 = tx3_input.tx.clone();

    // Add all transactions
    add_tx_naive(&mut mempool, &tx0_input);
    add_tx_naive(&mut mempool, &tx1_input);
    add_tx_naive(&mut mempool, &tx2_input);
    add_tx_naive(&mut mempool, &tx3_input);

    // Get only tx0 and tx1 (they're staged), tx2 and tx3 remain in queue
    get_txs_and_assert_expected_naive(&mut mempool, 3, &[tx0, tx1, tx2.clone()]);

    // Commit block: address 0x1 has next_nonce 2 (meaning nonces 0 and 1 were committed)
    // Expected: tx0 and tx1 are removed (committed), tx2 and tx3 remain in pool and queue
    // (they weren't staged, so they weren't rewound)
    commit_block_naive(&mut mempool, [("0x1", 2)], []);

    // Verify tx2 and tx3 are still in queue (they weren't staged, so they weren't rewound)
    get_txs_and_assert_expected_naive(&mut mempool, 2, &[tx2, tx3]);
}

#[test]
fn test_simple_rewind() {
    // Test 2: simple rewind - add_tx tx1, get_txs this tx, doesn't get it in commit_block, and
    // rewind it
    let mut mempool = create_mempool();
    let tx1_input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx1 = tx1_input.tx.clone();

    // Add tx1
    add_tx_naive(&mut mempool, &tx1_input);

    // Get tx1 (it's now staged)
    get_txs_and_assert_expected_naive(&mut mempool, 1, std::slice::from_ref(&tx1));

    // Commit block without committing tx1 (address not in committed list)
    // Expected: tx1 should be rewound back to queue
    commit_block_naive(&mut mempool, [], []);

    // Verify rewind - tx1 should be back in queue
    get_txs_and_assert_expected_naive(&mut mempool, 1, &[tx1]);
}

#[test]
fn test_complex_rewind_scenario() {
    // Test 3: complex rewind scenario
    // tx1, tx2, tx3, tx4 all staged, tx5 is in pool but is not staged
    // tx1 - committed
    // tx2 - not committed
    // tx3 - rejected
    // tx4 - not committed
    // Expected: tx2, tx3, tx4 are rewound
    // When we get_txs(4), we get tx2, tx3, tx4, tx5 in that order
    let mut mempool = create_mempool();
    let tx1_input = add_tx_input!(tx_hash: 1, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx2_input = add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx3_input = add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let tx4_input = add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 3, account_nonce: 0);
    let tx5_input = add_tx_input!(tx_hash: 5, address: "0x1", tx_nonce: 4, account_nonce: 0);

    let tx1 = tx1_input.tx.clone();
    let tx2 = tx2_input.tx.clone();
    let tx3 = tx3_input.tx.clone();
    let tx4 = tx4_input.tx.clone();
    let tx5 = tx5_input.tx.clone();

    // Add all transactions
    add_tx_naive(&mut mempool, &tx1_input);
    add_tx_naive(&mut mempool, &tx2_input);
    add_tx_naive(&mut mempool, &tx3_input);
    add_tx_naive(&mut mempool, &tx4_input);
    add_tx_naive(&mut mempool, &tx5_input);

    // Get tx1, tx2, tx3, tx4 (they're now staged), but NOT tx5
    get_txs_and_assert_expected_naive(
        &mut mempool,
        4,
        &[tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()],
    );

    // Commit block:
    // - tx1 is committed (next_nonce=1 means nonce 0 was committed)
    // - tx2 is not committed (next_nonce=1 means nonce 1 was NOT committed)
    // - tx3 is rejected
    // - tx4 is not committed
    // Expected: tx2, tx3, tx4 should be rewound (tx1 is committed so removed, tx2 is not committed
    // so address needs rewind, which means tx3 and tx4 are also rewound)
    commit_block_naive(&mut mempool, [("0x1", 1)], [tx_hash!(3)]);

    // Verify rewind - when we get_txs(4), we should get tx2, tx3, tx4, tx5 in that order
    // Note: tx2, tx3, tx4 are rewound in reverse order (newest first) and pushed to front,
    // so they end up in FIFO order: [tx2, tx3, tx4]
    // Then tx5 is already in queue, so final order is [tx2, tx3, tx4, tx5]
    get_txs_and_assert_expected_naive(&mut mempool, 4, &[tx2, tx3, tx4, tx5]);
}
