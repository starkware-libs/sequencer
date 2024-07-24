use std::cmp::Reverse;
use std::collections::HashMap;

use assert_matches::assert_matches;
use itertools::{enumerate, zip_eq};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_api::{contract_address, felt, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{Account, AccountState, ThinTransaction};
use starknet_types_core::felt::Felt;

use crate::mempool::{Mempool, MempoolInput, TransactionReference};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;

/// Represents the internal state of the mempool.
/// Enables customized (and potentially inconsistent) creation for unit testing.
struct MempoolState {
    tx_pool: TransactionPool,
    tx_queue: TransactionQueue,
}

impl MempoolState {
    fn new<PoolTxs, QueueTxs>(pool_txs: PoolTxs, queue_txs: QueueTxs) -> Self
    where
        PoolTxs: IntoIterator<Item = ThinTransaction>,
        QueueTxs: IntoIterator<Item = TransactionReference>,
    {
        let tx_pool: TransactionPool = pool_txs.into_iter().collect();
        let tx_queue: TransactionQueue = queue_txs.into_iter().collect();
        MempoolState { tx_pool, tx_queue }
    }

    fn assert_eq_mempool_state(&self, mempool: &Mempool) {
        assert_eq!(self.tx_pool, mempool.tx_pool);
        assert_eq!(self.tx_queue, mempool.tx_queue);
    }
}

impl From<MempoolState> for Mempool {
    fn from(mempool_state: MempoolState) -> Mempool {
        let MempoolState { tx_pool, tx_queue } = mempool_state;
        Mempool { tx_pool, tx_queue }
    }
}

impl FromIterator<ThinTransaction> for TransactionPool {
    fn from_iter<T: IntoIterator<Item = ThinTransaction>>(txs: T) -> Self {
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

/// Creates a valid input for mempool's `add_tx` with optional default values.
/// Usage:
/// 1. add_tx_input!(tip: 1, tx_hash: 2, sender_address: 3_u8, tx_nonce: 4, account_nonce: 3)
/// 2. add_tx_input!(tx_hash: 2, sender_address: 3_u8, tx_nonce: 4, account_nonce: 3)
/// 3. add_tx_input!(tip: 1, tx_hash: 2, sender_address: 3_u8)
/// 4. add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0)
/// 5. add_tx_input!(tip: 1, tx_hash: 2)
macro_rules! add_tx_input {
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {{
        let sender_address = contract_address!($sender_address);
        let account_nonce = Nonce(felt!($account_nonce));
        let account = Account { sender_address, state: AccountState {nonce: account_nonce}};
        let tx = ThinTransaction {
            tip: Tip($tip),
            tx_hash: TransactionHash(StarkHash::from($tx_hash)),
            sender_address,
            nonce: Nonce(felt!($tx_nonce)),
        };
        MempoolInput { tx, account }
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

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

// TODO(Ayelet): replace with MempoolState checker.
#[track_caller]
fn assert_eq_mempool_state(
    mempool: &Mempool,
    expected_pool: &[ThinTransaction],
    expected_queue: &[ThinTransaction],
) {
    assert_eq_mempool_queue(mempool, expected_queue);

    let expected_pool: HashMap<_, _> =
        expected_pool.iter().cloned().map(|tx| (tx.tx_hash, tx)).collect();
    assert_eq!(mempool._tx_pool()._tx_pool(), &expected_pool);
}

// Asserts that the transactions in the mempool are in ascending order as per the expected
// transactions.
#[track_caller]
fn assert_eq_mempool_queue(mempool: &Mempool, expected_queue: &[ThinTransaction]) {
    let mempool_txs = mempool.iter();
    let expected_queue = expected_queue.iter().map(TransactionReference::new);

    for (i, (expected_tx, mempool_tx)) in enumerate(zip_eq(expected_queue, mempool_txs)) {
        assert_eq!(expected_tx, *mempool_tx, "Transaction {i} in the queue is not as expected");
    }
}

#[rstest]
#[case::test_get_zero_txs(0)]
#[case::test_get_exactly_all_eligible_txs(3)]
#[case::test_get_more_than_all_eligible_txs(5)]
#[case::test_get_less_than_all_eligible_txs(2)]
fn test_get_txs(#[case] requested_txs: usize) {
    // Setup.
    let tx_tip_20_account_0 = add_tx_input!(tip: 20, tx_hash: 1, sender_address: "0x0").tx;
    let tx_tip_30_account_1 = add_tx_input!(tip: 30, tx_hash: 2, sender_address: "0x1").tx;
    let tx_tip_10_account_2 = add_tx_input!(tip: 10, tx_hash: 3, sender_address: "0x2").tx;

    let mut txs = vec![tx_tip_20_account_0, tx_tip_30_account_1, tx_tip_10_account_2];
    let tx_references_iterator = txs.iter().map(TransactionReference::new);
    let txs_iterator = txs.iter().cloned();

    let mut mempool: Mempool = MempoolState::new(txs_iterator, tx_references_iterator).into();

    // Test.
    let fetched_txs = mempool.get_txs(requested_txs).unwrap();

    txs.sort_by_key(|tx| Reverse(tx.tip));

    // Ensure we do not exceed the number of transactions available in the mempool.
    let max_requested_txs = requested_txs.min(txs.len());

    // Check that the returned transactions are the ones with the highest priority.
    let (expected_queue, remaining_txs) = txs.split_at(max_requested_txs);
    assert_eq!(fetched_txs, expected_queue);

    // Assert: non-returned transactions are still in the mempool.
    let remaining_tx_references = remaining_txs.iter().map(TransactionReference::new);
    let mempool_state = MempoolState::new(remaining_txs.to_vec(), remaining_tx_references);
    mempool_state.assert_eq_mempool_state(&mempool);
}

#[rstest]
// TODO(Ayelet): remove ignore once replenishing is merged.
#[ignore]
fn test_get_txs_multi_nonce() {
    // Setup.
    let tx_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8).tx;
    let tx_address_0_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8).tx;

    let queue_txs = [TransactionReference::new(&tx_address_0_nonce_0)];
    let pool_txs = [tx_address_0_nonce_0.clone(), tx_address_0_nonce_1.clone()];
    let mut mempool: Mempool = MempoolState::new(pool_txs, queue_txs).into();

    // Test.
    let txs = mempool.get_txs(2).unwrap();

    // Assert that the account's next tx was added the queue.
    assert_eq!(txs, &[tx_address_0_nonce_0, tx_address_0_nonce_1]);
    let expected_mempool_state = MempoolState::new([], []);
    expected_mempool_state.assert_eq_mempool_state(&mempool);
}

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
    add_tx_inputs.sort_by_key(|input| std::cmp::Reverse(input.tx.tip));

    // Assert: transactions are ordered by priority.
    let expected_queue_txs: Vec<TransactionReference> =
        add_tx_inputs.iter().map(|input| TransactionReference::new(&input.tx)).collect();
    let expected_pool_txs = add_tx_inputs.into_iter().map(|input| input.tx);
    let expected_mempool_state = MempoolState::new(expected_pool_txs, expected_queue_txs);
    expected_mempool_state.assert_eq_mempool_state(&mempool);
}

#[rstest]
fn test_add_tx_multi_nonce_success(mut mempool: Mempool) {
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_address_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x1", tx_nonce: 0_u8,account_nonce: 0_u8);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 3, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8);

    add_tx(&mut mempool, &input_address_0_nonce_0);
    add_tx(&mut mempool, &input_address_1);
    add_tx(&mut mempool, &input_address_0_nonce_1);

    let expected_pool_all_txs = &[
        input_address_0_nonce_0.tx.clone(),
        input_address_1.tx.clone(),
        input_address_0_nonce_1.tx,
    ];
    let expected_queue_only_zero_nonce_txs = &[input_address_1.tx, input_address_0_nonce_0.tx];

    assert_eq_mempool_state(&mempool, expected_pool_all_txs, expected_queue_only_zero_nonce_txs);
}

#[test]
fn test_new_with_duplicate_tx() {
    let input = add_tx_input!(tip: 0, tx_hash: 1);
    let same_input = input.clone();

    assert!(matches!(
        Mempool::new([input, same_input]),
        Err(MempoolError::DuplicateTransaction { .. })
    ));
}

#[rstest]
fn test_add_tx_with_duplicate_tx(mut mempool: Mempool) {
    let input = add_tx_input!(tip: 50, tx_hash: Felt::ONE);
    let same_input = input.clone();

    add_tx(&mut mempool, &input);

    assert_matches!(
        mempool.add_tx(same_input.clone()),
        Err(MempoolError::DuplicateTransaction { .. })
    );
    // Assert that the original tx remains in the pool after the failed attempt.
    assert_eq_mempool_queue(&mempool, &[same_input.tx])
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
    let expected_mempool_state = MempoolState::new(expected_pool_txs, expected_queue_txs);

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    expected_mempool_state.assert_eq_mempool_state(&mempool);
}

#[rstest]
fn test_tip_priority_over_tx_hash(mut mempool: Mempool) {
    let input_big_tip_small_hash = add_tx_input!(tip: 2, tx_hash: Felt::ONE);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash = add_tx_input!(tip: 1, tx_hash: Felt::TWO, sender_address: "0x1");

    add_tx(&mut mempool, &input_big_tip_small_hash);
    add_tx(&mut mempool, &input_small_tip_big_hash);
    assert_eq_mempool_queue(&mempool, &[input_big_tip_small_hash.tx, input_small_tip_big_hash.tx])
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
    let mut mempool: Mempool = MempoolState::new(pool_txs, queue_txs).into();

    // Test.
    let txs = mempool.get_txs(2).unwrap();

    // Assert.
    assert_eq!(txs, &[tx_address_1_nonce_0]);

    let expected_pool_txs = [tx_address_0_nonce_1];
    let expected_queue_txs = [];
    let mempool_state = MempoolState::new(expected_pool_txs, expected_queue_txs);
    mempool_state.assert_eq_mempool_state(&mempool);
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
    let expected_mempool_state = MempoolState::new(expected_pool_txs, expected_queue_txs);

    expected_mempool_state.assert_eq_mempool_state(&mempool);
}

#[rstest]
fn test_get_txs_with_holes_single_account() {
    // Setup.
    let input_nonce_1 = add_tx_input!(tx_hash: 0, tx_nonce: 1_u8, account_nonce: 0_u8);

    let pool_txs = [input_nonce_1.tx];
    let queue_txs = [];
    let mut mempool: Mempool = MempoolState::new(pool_txs.clone(), queue_txs).into();

    // Test.
    let txs = mempool.get_txs(1).unwrap();

    // Assert.
    assert_eq!(txs, &[]);

    let mempool_state = MempoolState::new(pool_txs, queue_txs);
    mempool_state.assert_eq_mempool_state(&mempool);
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
    let expected_mempool_state = MempoolState::new(expected_pool_txs, expected_queue_txs);
    expected_mempool_state.assert_eq_mempool_state(&mempool);

    // Test: add the first transaction, which fills the hole.
    add_tx(&mut mempool, &input_nonce_0);

    // Assert: only the eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_1.tx, input_nonce_0.tx];
    let expected_mempool_state = MempoolState::new(expected_pool_txs, expected_queue_txs);
    expected_mempool_state.assert_eq_mempool_state(&mempool);
}

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

    // TODO(Ayelet): all transactions should be returned after replenishing.
    // Assert: all remaining transactions are returned.
    assert_eq!(txs, &[input_address_0_nonce_0.tx]);
}

#[rstest]
#[ignore]

fn test_commit_block_rewinds_nonce() {
    // Setup.
    let tx_address0_nonce5 = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8, account_nonce: 4_u8).tx;

    let queued_txs = [TransactionReference::new(&tx_address0_nonce5)];
    let pool_txs = [tx_address0_nonce5];
    let mut mempool: Mempool = MempoolState::new(pool_txs, queued_txs).into();

    // Test.
    let state_changes = HashMap::from([
        (contract_address!("0x0"), AccountState { nonce: Nonce(felt!(3_u16)) }),
        (contract_address!("0x1"), AccountState { nonce: Nonce(felt!(3_u16)) }),
    ]);
    assert!(mempool.commit_block(state_changes).is_ok());

    // Assert.
    assert_eq_mempool_queue(&mempool, &[])
}
