use std::cmp::Reverse;
use std::collections::HashMap;

use assert_matches::assert_matches;
use mempool_test_utils::starknet_api_test_utils::{
    create_executable_tx,
    test_resource_bounds_mapping,
};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, Nonce, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_api::{contract_address, felt, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{Account, AccountState};
use starknet_types_core::felt::Felt;

use crate::mempool::{Mempool, MempoolInput, TransactionReference};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;

// Utils.

/// Represents the internal content of the mempool.
/// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug)]
struct MempoolContent<T> {
    tx_pool: Option<TransactionPool>,
    tx_queue: Option<TransactionQueue>,
    // Artificially use generic type, for the compiler.
    _phantom: std::marker::PhantomData<T>,
}

#[derive(Debug)]
struct PartialContent;

impl MempoolContent<PartialContent> {
    fn with_pool_and_queue<P, Q>(pool_txs: P, queue_txs: Q) -> Self
    where
        P: IntoIterator<Item = Transaction>,
        // TODO(Ayelet): Consider using `&ThinTransaction` instead of `TransactionReference`.
        Q: IntoIterator<Item = TransactionReference>,
    {
        Self {
            tx_pool: Some(pool_txs.into_iter().collect()),
            tx_queue: Some(queue_txs.into_iter().collect()),
            _phantom: std::marker::PhantomData,
        }
    }

    fn with_pool<P>(pool_txs: P) -> Self
    where
        P: IntoIterator<Item = Transaction>,
    {
        Self {
            tx_pool: Some(pool_txs.into_iter().collect()),
            tx_queue: None,
            _phantom: std::marker::PhantomData,
        }
    }

    fn with_queue<Q>(queue_txs: Q) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
    {
        Self {
            tx_queue: Some(queue_txs.into_iter().collect()),
            tx_pool: None,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> MempoolContent<T> {
    fn assert_eq_pool_and_queue_content(&self, mempool: &Mempool) {
        self.assert_eq_pool_content(mempool);
        self.assert_eq_queue_content(mempool);
    }

    fn assert_eq_pool_content(&self, mempool: &Mempool) {
        assert_eq!(self.tx_pool.as_ref().unwrap(), &mempool.tx_pool);
    }

    fn assert_eq_queue_content(&self, mempool: &Mempool) {
        assert_eq!(self.tx_queue.as_ref().unwrap(), &mempool.tx_queue);
    }
}

impl<T> From<MempoolContent<T>> for Mempool {
    fn from(mempool_content: MempoolContent<T>) -> Mempool {
        let MempoolContent { tx_pool, tx_queue, _phantom: _ } = mempool_content;
        Mempool {
            tx_pool: tx_pool.unwrap_or_default(),
            tx_queue: tx_queue.unwrap_or_default(),
            // TODO: Add implementation when needed.
            mempool_state: Default::default(),
        }
    }
}

impl FromIterator<Transaction> for TransactionPool {
    fn from_iter<T: IntoIterator<Item = Transaction>>(txs: T) -> Self {
        let mut pool = Self::default();
        for tx in txs {
            pool.insert(tx).unwrap();
        }
        pool
    }
}

impl FromIterator<TransactionReference> for TransactionQueue {
    fn from_iter<T: IntoIterator<Item = TransactionReference>>(txs: T) -> Self {
        let mut queue = Self::default();
        for tx in txs {
            queue.insert(tx);
        }
        queue
    }
}

#[track_caller]
fn add_tx(mempool: &mut Mempool, input: &MempoolInput) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

#[track_caller]
fn add_tx_expect_error(mempool: &mut Mempool, input: &MempoolInput, expected_error: MempoolError) {
    assert_eq!(mempool.add_tx(input.clone()), Err(expected_error));
}

/// Creates a valid input for mempool's `add_tx` with optional default values.
/// Usage:
/// 1. add_tx_input!(tip: 1, tx_hash: 2, sender_address: 3_u8, tx_nonce: 4, account_nonce: 3)
/// 2. add_tx_input!(tx_hash: 2, sender_address: 3_u8, tx_nonce: 4, account_nonce: 3)
/// 3. add_tx_input!(tip: 1, tx_hash: 2, sender_address: 3_u8)
/// 4. add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0)
/// 5. add_tx_input!(tip: 1, tx_hash: 2)
macro_rules! add_tx_input {
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr, resource_bounds: $resource_bounds:expr) => {{
        let sender_address = contract_address!($sender_address);
        let account_nonce = Nonce(felt!($account_nonce));
        let account = Account { sender_address, state: AccountState {nonce: account_nonce}};

        let tx = create_executable_tx(
            sender_address,
            TransactionHash(StarkHash::from($tx_hash)),
            Tip($tip),
            Nonce(felt!($tx_nonce)),
            $resource_bounds,
        );
        MempoolInput { tx, account }
    }};
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {{
            add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce, resource_bounds: test_resource_bounds_mapping().into())
    }};
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0_u8, account_nonce: 0_u8)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8)
    };
}

// Fixtures.

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

// Tests.

// get_txs tests.

#[rstest]
#[case::test_get_zero_txs(0)]
#[case::test_get_exactly_all_eligible_txs(3)]
#[case::test_get_more_than_all_eligible_txs(5)]
#[case::test_get_less_than_all_eligible_txs(2)]
fn test_get_txs_returns_by_priority_order(#[case] requested_txs: usize) {
    // Setup.
    let tx_tip_20_account_0 = add_tx_input!(tip: 20, tx_hash: 1, sender_address: "0x0").tx;
    let tx_tip_30_account_1 = add_tx_input!(tip: 30, tx_hash: 2, sender_address: "0x1").tx;
    let tx_tip_10_account_2 = add_tx_input!(tip: 10, tx_hash: 3, sender_address: "0x2").tx;

    let mut txs = vec![tx_tip_20_account_0, tx_tip_30_account_1, tx_tip_10_account_2];
    let tx_references_iterator = txs.iter().map(TransactionReference::new);
    let txs_iterator = txs.iter().cloned();

    let mut mempool: Mempool =
        MempoolContent::with_pool_and_queue(txs_iterator, tx_references_iterator).into();

    // Test.
    let fetched_txs = mempool.get_txs(requested_txs).unwrap();

    txs.sort_by_key(|tx| Reverse(tx.tip()));

    // Ensure we do not exceed the number of transactions available in the mempool.
    let max_requested_txs = requested_txs.min(txs.len());

    // Check that the returned transactions are the ones with the highest priority.
    let (expected_queue, remaining_txs) = txs.split_at(max_requested_txs);
    assert_eq!(fetched_txs, expected_queue);

    // Assert: non-returned transactions are still in the mempool.
    let remaining_tx_references = remaining_txs.iter().map(TransactionReference::new);
    let mempool_content =
        MempoolContent::with_pool_and_queue(remaining_txs.to_vec(), remaining_tx_references);
    mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_multi_nonce() {
    // Setup.
    let tx_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8).tx;
    let tx_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8).tx;
    let tx_nonce_2 =
        add_tx_input!(tx_hash: 3, sender_address: "0x0", tx_nonce: 2_u8, account_nonce: 0_u8).tx;

    let queue_txs = [&tx_nonce_0].map(TransactionReference::new);
    let pool_txs = [tx_nonce_0, tx_nonce_1, tx_nonce_2];
    let mut mempool: Mempool =
        MempoolContent::with_pool_and_queue(pool_txs.clone(), queue_txs).into();

    // Test.
    let fetched_txs = mempool.get_txs(3).unwrap();

    // Assert: all transactions are returned.
    assert_eq!(fetched_txs, &pool_txs);
    let expected_mempool_content = MempoolContent::with_pool_and_queue([], []);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_replenishes_queue_only_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 =
        add_tx_input!(tip: 20, tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8).tx;
    let tx_address_0_nonce_1 =
        add_tx_input!(tip: 20, tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8).tx;
    let tx_address_1_nonce_0 =
        add_tx_input!(tip: 10, tx_hash: 3, sender_address: "0x1", tx_nonce: 0_u8, account_nonce: 0_u8).tx;

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs =
        [&tx_address_0_nonce_0, &tx_address_0_nonce_1, &tx_address_1_nonce_0].map(|tx| tx.clone());
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.
    let txs = mempool.get_txs(3).unwrap();

    // Assert: all transactions returned.
    // Replenishment done in chunks: account 1 transaction is returned before the one of account 0,
    // although its priority is higher.
    assert_eq!(txs, &[tx_address_0_nonce_0, tx_address_1_nonce_0, tx_address_0_nonce_1]);
    let expected_mempool_content = MempoolContent::with_pool_and_queue([], []);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_replenishes_queue_multi_account_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 =
        add_tx_input!(tip: 30, tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8).tx;
    let tx_address_1_nonce_0 =
        add_tx_input!(tip: 20, tx_hash: 2, sender_address: "0x1", tx_nonce: 0_u8, account_nonce: 0_u8).tx;
    let tx_address_0_nonce_1 =
        add_tx_input!(tip: 30, tx_hash: 3, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8).tx;
    let tx_address_1_nonce_1 =
        add_tx_input!(tip: 20, tx_hash: 4, sender_address: "0x1", tx_nonce: 1_u8, account_nonce: 0_u8).tx;

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs = [
        &tx_address_0_nonce_0,
        &tx_address_1_nonce_0,
        &tx_address_0_nonce_1,
        &tx_address_1_nonce_1,
    ]
    .map(|tx| tx.clone());
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.
    let txs = mempool.get_txs(2).unwrap();

    // Assert.
    assert_eq!(txs, [tx_address_0_nonce_0, tx_address_1_nonce_0]);

    // Queue is replenished with the next transactions of each account.
    let expected_queue_txs =
        [&tx_address_0_nonce_1, &tx_address_1_nonce_1].map(TransactionReference::new);
    let expected_pool_txs = [tx_address_0_nonce_1, tx_address_1_nonce_1];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_with_holes_multiple_accounts() {
    // Setup.
    let tx_address_0_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8).tx;
    let tx_address_1_nonce_0 =
        add_tx_input!(tx_hash: 3, sender_address: "0x1", tx_nonce: 0_u8, account_nonce: 0_u8).tx;

    let queue_txs = [TransactionReference::new(&tx_address_1_nonce_0)];
    let pool_txs = [tx_address_0_nonce_1.clone(), tx_address_1_nonce_0.clone()];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.
    let txs = mempool.get_txs(2).unwrap();

    // Assert.
    assert_eq!(txs, &[tx_address_1_nonce_0]);

    let expected_pool_txs = [tx_address_0_nonce_1];
    let expected_queue_txs = [];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_with_holes_single_account() {
    // Setup.
    let input_nonce_1 = add_tx_input!(tx_hash: 0, tx_nonce: 1_u8, account_nonce: 0_u8);

    let pool_txs = [input_nonce_1.tx];
    let queue_txs = [];
    let mut mempool: Mempool =
        MempoolContent::with_pool_and_queue(pool_txs.clone(), queue_txs.clone()).into();

    // Test.
    let txs = mempool.get_txs(1).unwrap();

    // Assert.
    assert_eq!(txs, &[]);

    let expected_mempool_content = MempoolContent::with_pool_and_queue(pool_txs, queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

// add_tx tests.

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    // Setup.
    let mut add_tx_inputs = [
        add_tx_input!(tip: 50, tx_hash: 1, sender_address: "0x0"),
        add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1"),
        add_tx_input!(tip: 80, tx_hash: 3, sender_address: "0x2"),
    ];

    // Test.
    for input in &add_tx_inputs {
        add_tx(&mut mempool, input);
    }

    // TODO(Ayelet): Consider share this code.
    // Sort in an ascending priority order.
    add_tx_inputs.sort_by_key(|input| std::cmp::Reverse(input.tx.tip().unwrap()));

    // Assert: transactions are ordered by priority.
    let expected_queue_txs: Vec<TransactionReference> =
        add_tx_inputs.iter().map(|input| TransactionReference::new(&input.tx)).collect();
    let expected_pool_txs = add_tx_inputs.into_iter().map(|input| input.tx);
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_multi_nonce_success(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 2, sender_address: "0x1", tx_nonce: 0_u8,account_nonce: 0_u8);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 3, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8);

    // Test.
    add_tx(&mut mempool, &input_address_0_nonce_0);
    add_tx(&mut mempool, &input_address_1_nonce_0);
    add_tx(&mut mempool, &input_address_0_nonce_1);

    // Assert: only the eligible transactions appear in the queue.
    let expected_queue_txs =
        [&input_address_1_nonce_0.tx, &input_address_0_nonce_0.tx].map(TransactionReference::new);
    let expected_pool_txs =
        [input_address_0_nonce_0.tx, input_address_1_nonce_0.tx, input_address_0_nonce_1.tx];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_with_duplicate_tx(mut mempool: Mempool) {
    // Setup.
    let input = add_tx_input!(tip: 50, tx_hash: Felt::ONE);
    let duplicate_input = input.clone();

    // Test.
    add_tx(&mut mempool, &input);
    assert_matches!(
        mempool.add_tx(duplicate_input),
        Err(MempoolError::DuplicateTransaction { .. })
    );

    // Assert: the original transaction remains.
    let expected_mempool_content = MempoolContent::with_pool([input.tx]);
    expected_mempool_content.assert_eq_pool_content(&mempool);
}

#[rstest]
fn test_add_tx_lower_than_queued_nonce() {
    // Setup.
    let valid_input =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 1_u8);
    let lower_nonce_input =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);

    let queue_txs = [TransactionReference::new(&valid_input.tx)];
    let expected_mempool_content = MempoolContent::with_queue(queue_txs.clone());
    let pool_txs = [valid_input.tx];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test and assert the original transaction remains.
    assert_matches!(mempool.add_tx(lower_nonce_input), Err(MempoolError::DuplicateNonce { .. }));
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_updates_queue_with_higher_account_nonce() {
    // Setup.
    let input =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let higher_account_nonce_input =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 1_u8);

    let queue_txs = [TransactionReference::new(&input.tx)];
    let mut mempool: Mempool = MempoolContent::with_queue(queue_txs).into();

    // Test.
    add_tx(&mut mempool, &higher_account_nonce_input);

    // Assert: the higher account nonce transaction is in the queue.
    let expected_queue_txs = [TransactionReference::new(&higher_account_nonce_input.tx)];
    let expected_mempool_content = MempoolContent::with_queue(expected_queue_txs);
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_with_identical_tip_succeeds(mut mempool: Mempool) {
    // Setup.
    let input1 = add_tx_input!(tip: 1, tx_hash: 2);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(tip: 1, tx_hash: 1, sender_address: "0x1");

    // Test.
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // Assert: both transactions are in the mempool.
    let expected_queue_txs =
        [TransactionReference::new(&input1.tx), TransactionReference::new(&input2.tx)];
    let expected_pool_txs = [input1.tx, input2.tx];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_delete_tx_with_lower_nonce_than_account_nonce() {
    // Setup.
    let tx_nonce_0_account_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let tx_nonce_1_account_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 1_u8);

    let queue_txs = [TransactionReference::new(&tx_nonce_0_account_nonce_0.tx)];
    let pool_txs = [tx_nonce_0_account_nonce_0.tx];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.
    add_tx(&mut mempool, &tx_nonce_1_account_nonce_1);

    // Assert the transaction with the lower nonce is removed.
    let expected_queue_txs = [TransactionReference::new(&tx_nonce_1_account_nonce_1.tx)];
    let expected_pool_txs = [tx_nonce_1_account_nonce_1.tx];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_tip_priority_over_tx_hash(mut mempool: Mempool) {
    // Setup.
    let input_big_tip_small_hash = add_tx_input!(tip: 2, tx_hash: Felt::ONE);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash = add_tx_input!(tip: 1, tx_hash: Felt::TWO, sender_address: "0x1");

    // Test.
    add_tx(&mut mempool, &input_big_tip_small_hash);
    add_tx(&mut mempool, &input_small_tip_big_hash);

    // Assert: ensure that the transaction with the higher tip is prioritized higher.
    let expected_queue_txs =
        [&input_big_tip_small_hash.tx, &input_small_tip_big_hash.tx].map(TransactionReference::new);
    let expected_mempool_content = MempoolContent::with_queue(expected_queue_txs);
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_account_state_fills_hole(mut mempool: Mempool) {
    // Setup.
    let tx_input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1_u8, account_nonce: 0_u8);
    // Input that increments the account state.
    let tx_input_nonce_2 = add_tx_input!(tx_hash: 2, tx_nonce: 2_u8, account_nonce: 1_u8);

    // Test and assert.

    // First, with gap.
    add_tx(&mut mempool, &tx_input_nonce_1);
    let expected_mempool_content = MempoolContent::with_queue([]);
    expected_mempool_content.assert_eq_queue_content(&mempool);

    // Then, fill it.
    add_tx(&mut mempool, &tx_input_nonce_2);
    let expected_queue_txs = [&tx_input_nonce_1.tx].map(TransactionReference::new);
    let expected_mempool_content = MempoolContent::with_queue(expected_queue_txs);
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_sequential_nonces(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 0, tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1_u8, account_nonce: 0_u8);

    // Test.
    add_tx(&mut mempool, &input_nonce_0);
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: only eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_0.tx, input_nonce_1.tx];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);

    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_filling_hole(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 1, tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_nonce_1 = add_tx_input!(tx_hash: 2, tx_nonce: 1_u8, account_nonce: 0_u8);

    // Test: add the second transaction first, which creates a hole in the sequence.
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: the second transaction is in the pool and not in the queue.
    let expected_queue_txs = [];
    let expected_pool_txs = [input_nonce_1.tx.clone()];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);

    // Test: add the first transaction, which fills the hole.
    add_tx(&mut mempool, &input_nonce_0);

    // Assert: only the eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_1.tx, input_nonce_0.tx];
    let expected_mempool_content =
        MempoolContent::with_pool_and_queue(expected_pool_txs, expected_queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

// commit_block tests.

#[rstest]
fn test_add_tx_after_get_txs_fails_on_duplicate_nonce() {
    // Setup.
    let input_tx = add_tx_input!(tip: 1, tx_hash: Felt::ZERO);

    let pool_txs = [input_tx.tx.clone()];
    let queue_txs = [TransactionReference::new(&input_tx.tx)];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.
    mempool.get_txs(1).unwrap();
    add_tx_expect_error(
        &mut mempool,
        &input_tx,
        MempoolError::DuplicateNonce {
            address: contract_address!("0x0"),
            nonce: Nonce(felt!(0_u16)),
        },
    );
}

#[rstest]
fn test_commit_block_includes_all_txs() {
    // Setup.
    let tx_address0_nonce4 = add_tx_input!(tip: 4, tx_hash: 1, sender_address: "0x0", tx_nonce: 4_u8, account_nonce: 4_u8).tx;
    let tx_address0_nonce5 = add_tx_input!(tip: 3, tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 4_u8).tx;
    let tx_address1_nonce3 = add_tx_input!(tip: 2, tx_hash: 3, sender_address: "0x1", tx_nonce: 3_u8, account_nonce: 3_u8).tx;
    let tx_address2_nonce1 = add_tx_input!(tip: 1, tx_hash: 4, sender_address: "0x2", tx_nonce: 1_u8, account_nonce: 1_u8).tx;

    let queue_txs = [&tx_address0_nonce4, &tx_address1_nonce3, &tx_address2_nonce1]
        .map(TransactionReference::new);
    let pool_txs = [tx_address0_nonce4, tx_address0_nonce5, tx_address1_nonce3, tx_address2_nonce1];
    let mut mempool: Mempool =
        MempoolContent::with_pool_and_queue(pool_txs.clone(), queue_txs.clone()).into();

    // Test.
    let state_changes = HashMap::from([
        (contract_address!("0x0"), AccountState { nonce: Nonce(felt!(3_u16)) }),
        (contract_address!("0x1"), AccountState { nonce: Nonce(felt!(2_u16)) }),
    ]);
    assert!(mempool.commit_block(state_changes).is_ok());

    // Assert.
    let expected_mempool_content = MempoolContent::with_pool_and_queue(pool_txs, queue_txs);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_commit_block_rewinds_nonce() {
    // Setup.
    let tx_address0_nonce5 = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 4_u8).tx;

    let queued_txs = [TransactionReference::new(&tx_address0_nonce5)];
    let pool_txs = [tx_address0_nonce5];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queued_txs).into();

    // Test.
    let state_changes = HashMap::from([
        (contract_address!("0x0"), AccountState { nonce: Nonce(felt!(3_u16)) }),
        (contract_address!("0x1"), AccountState { nonce: Nonce(felt!(3_u16)) }),
    ]);
    assert!(mempool.commit_block(state_changes).is_ok());

    // Assert.
    let expected_mempool_content = MempoolContent::with_queue([]);
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

#[rstest]
fn test_commit_block_from_different_leader() {
    // Setup.
    let tx_address0_nonce3 = add_tx_input!(tip: 1, tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8, account_nonce: 2_u8).tx;
    let tx_address0_nonce5 = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 2_u8).tx;
    let tx_address0_nonce6 = add_tx_input!(tip: 1, tx_hash: 3, sender_address: "0x0", tx_nonce: 6_u8, account_nonce: 2_u8).tx;
    let tx_address1_nonce2 = add_tx_input!(tip: 1, tx_hash: 4, sender_address: "0x1", tx_nonce: 2_u8, account_nonce: 2_u8).tx;

    let queued_txs = [TransactionReference::new(&tx_address1_nonce2)];
    let pool_txs =
        [tx_address0_nonce3, tx_address0_nonce5, tx_address0_nonce6.clone(), tx_address1_nonce2];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queued_txs).into();

    // Test.
    let state_changes = HashMap::from([
        (contract_address!("0x0"), AccountState { nonce: Nonce(felt!(5_u16)) }),
        // A hole, missing nonce 1.
        (contract_address!("0x1"), AccountState { nonce: Nonce(felt!(0_u16)) }),
        (contract_address!("0x2"), AccountState { nonce: Nonce(felt!(1_u16)) }),
    ]);
    assert!(mempool.commit_block(state_changes).is_ok());

    // Assert.
    let expected_queue_txs = [&tx_address0_nonce6].map(TransactionReference::new);
    let expected_mempool_content = MempoolContent::with_queue(expected_queue_txs);
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

// Flow tests.

#[rstest]
fn test_flow_filling_holes(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 3, sender_address: "0x1", tx_nonce: 0_u8, account_nonce: 0_u8);

    // Test.
    add_tx(&mut mempool, &input_address_0_nonce_1);
    add_tx(&mut mempool, &input_address_1_nonce_0);
    let txs = mempool.get_txs(2).unwrap();

    // Assert: only the eligible transaction is returned.
    assert_eq!(txs, &[input_address_1_nonce_0.tx]);

    // Test.
    add_tx(&mut mempool, &input_address_0_nonce_0);
    let txs = mempool.get_txs(2).unwrap();

    // Assert: all remaining transactions are returned.
    assert_eq!(txs, &[input_address_0_nonce_0.tx, input_address_0_nonce_1.tx]);
}

#[rstest]
fn test_flow_partial_commit_block() {
    // Setup.
    let tx_address0_nonce3 =
        add_tx_input!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8, account_nonce: 3_u8).tx;
    let tx_address0_nonce5 =
        add_tx_input!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 3_u8).tx;
    let tx_address0_nonce6 =
        add_tx_input!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 6_u8, account_nonce: 3_u8).tx;
    let tx_address1_nonce0 =
        add_tx_input!(tip: 20, tx_hash: 4, sender_address: "0x1", tx_nonce: 0_u8, account_nonce: 0_u8).tx;
    let tx_address1_nonce1 =
        add_tx_input!(tip: 21, tx_hash: 5, sender_address: "0x1", tx_nonce: 1_u8, account_nonce: 0_u8).tx;
    let tx_address1_nonce2 =
        add_tx_input!(tip: 22, tx_hash: 6, sender_address: "0x1", tx_nonce: 2_u8, account_nonce: 0_u8).tx;
    let tx_address2_nonce2 =
        add_tx_input!(tip: 0, tx_hash: 7, sender_address: "0x2", tx_nonce: 2_u8, account_nonce: 2_u8).tx;

    let queue_txs = [&tx_address0_nonce3, &tx_address1_nonce0, &tx_address2_nonce2]
        .map(TransactionReference::new);
    let pool_txs = [
        &tx_address0_nonce3,
        &tx_address0_nonce5,
        &tx_address0_nonce6,
        &tx_address1_nonce0,
        &tx_address1_nonce1,
        &tx_address1_nonce2,
        &tx_address2_nonce2,
    ]
    .map(|tx| tx.clone());
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.

    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs, &[tx_address1_nonce0, tx_address0_nonce3]);

    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs, &[tx_address1_nonce1, tx_address2_nonce2]);

    // Not included in block: `tx_address2_nonce2`, `tx_address1_nonce1`.
    let state_changes = HashMap::from([
        (contract_address!("0x0"), AccountState { nonce: Nonce(felt!(3_u16)) }),
        (contract_address!("0x1"), AccountState { nonce: Nonce(felt!(0_u16)) }),
    ]);
    assert!(mempool.commit_block(state_changes).is_ok());

    // Assert.
    let expected_pool_txs = [tx_address0_nonce5, tx_address0_nonce6, tx_address1_nonce2];
    let expected_mempool_content = MempoolContent::with_pool_and_queue(expected_pool_txs, []);
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_flow_commit_block_closes_hole() {
    // Setup.
    let tx_nonce3 =
        add_tx_input!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8, account_nonce: 3_u8).tx;
    let tx_input_nonce4 = add_tx_input!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 4_u8, account_nonce: 5_u8);
    let tx_nonce5 =
        add_tx_input!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 3_u8).tx;

    let queued_txs = [TransactionReference::new(&tx_nonce3)];
    let pool_txs = [tx_nonce3, tx_nonce5.clone()];
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queued_txs).into();

    // Test.
    let state_changes =
        HashMap::from([(contract_address!("0x0"), AccountState { nonce: Nonce(felt!(4_u8)) })]);
    assert!(mempool.commit_block(state_changes).is_ok());

    // Assert: hole was indeed closed.
    let expected_queue_txs = [&tx_nonce5].map(TransactionReference::new);
    let expected_mempool_content = MempoolContent::with_queue(expected_queue_txs);
    expected_mempool_content.assert_eq_queue_content(&mempool);

    let res = mempool.add_tx(tx_input_nonce4);
    assert_eq!(
        res,
        Err(MempoolError::DuplicateNonce {
            address: contract_address!("0x0"),
            nonce: Nonce(felt!(4_u8)),
        })
    );
}

#[rstest]
fn test_flow_send_same_nonce_tx_after_previous_not_included() {
    // Setup.
    let tx_nonce3 =
        add_tx_input!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8, account_nonce: 3_u8).tx;
    let tx_input_nonce4 = add_tx_input!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 4_u8, account_nonce: 4_u8);
    let tx_nonce5 =
        add_tx_input!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 3_u8).tx;

    let queue_txs = [TransactionReference::new(&tx_nonce3)];
    let pool_txs = [&tx_nonce3, &tx_input_nonce4.tx, &tx_nonce5].map(|tx| tx.clone());
    let mut mempool: Mempool = MempoolContent::with_pool_and_queue(pool_txs, queue_txs).into();

    // Test.
    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs, &[tx_nonce3, tx_input_nonce4.tx.clone()]);

    // Transaction with nonce 4 is not included in the block.
    let state_changes =
        HashMap::from([(contract_address!("0x0"), AccountState { nonce: Nonce(felt!(3_u16)) })]);
    assert!(mempool.commit_block(state_changes).is_ok());

    add_tx(&mut mempool, &tx_input_nonce4);
    let txs = mempool.get_txs(1).unwrap();

    // Assert.
    assert_eq!(txs, &[tx_input_nonce4.tx]);
    let expected_queue_txs = [TransactionReference::new(&tx_nonce5)];
    let expected_mempool_content = MempoolContent::with_queue(expected_queue_txs);
    expected_mempool_content.assert_eq_queue_content(&mempool);
}

#[rstest]
fn test_tx_pool_capacity(mut mempool: Mempool) {
    let input_1 =
        add_tx_input!(tx_hash: 0, sender_address: 0_u8, tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_2 =
        add_tx_input!(tx_hash: 1, sender_address: 1_u8, tx_nonce: 0_u8, account_nonce: 0_u8);

    // Test and assert: add txs to the counter.
    add_tx(&mut mempool, &input_1);
    add_tx(&mut mempool, &input_2);
    assert_eq!(mempool.tx_pool().n_txs(), 2);

    // Test and assert: duplicate transaction doesn't affect capacity.
    add_tx_expect_error(
        &mut mempool,
        &input_1,
        MempoolError::DuplicateTransaction { tx_hash: input_1.tx.tx_hash() },
    );
    assert_eq!(mempool.tx_pool().n_txs(), 2);

    // Test and assert: updates pool capacity when a transaction is removed upon receiving state
    // changes.
    let state_changes =
        HashMap::from([(contract_address!("0x0"), AccountState { nonce: Nonce(felt!(4_u8)) })]);
    assert!(mempool.commit_block(state_changes).is_ok());
    assert_eq!(mempool.tx_pool().n_txs(), 1);

    // Test and assert: remove the transactions, counter does not go below 0.
    mempool.get_txs(2).unwrap();
    assert_eq!(mempool.tx_pool().n_txs(), 0);
}
