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
/// 4. add_tx_input!(tip: 1, tx_hash: 2)
macro_rules! add_tx_input {
    // Pattern for all four arguments with keyword arguments.
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
    // Pattern for four arguments.
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    // Pattern for three arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0_u8, account_nonce: 0_u8)
    };
    // Pattern for two arguments.
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
    // TODO(Ayelet): Avoid cloning the transactions in the test.
    let add_tx_inputs = [
        add_tx_input!(tip: 50, tx_hash: 1),
        add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1"),
        add_tx_input!(tip: 10, tx_hash: 3, sender_address: "0x2"),
    ];
    let tx_references_iterator =
        add_tx_inputs.iter().map(|input| TransactionReference::new(&input.tx));
    let txs_iterator = add_tx_inputs.iter().map(|input| input.tx.clone());

    let mut mempool: Mempool = MempoolState::new(txs_iterator, tx_references_iterator).into();

    let txs = mempool.get_txs(requested_txs).unwrap();

    let sorted_txs = [
        add_tx_inputs[1].tx.clone(), // tip 100
        add_tx_inputs[0].tx.clone(), // tip 50
        add_tx_inputs[2].tx.clone(), // tip 10
    ];

    // This ensures we do not exceed the number of transactions available in the mempool.
    let max_requested_txs = requested_txs.min(add_tx_inputs.len());

    // checks that the returned transactions are the ones with the highest priority.
    let (expected_queue, remaining_txs) = sorted_txs.split_at(max_requested_txs);
    assert_eq!(txs, expected_queue);

    // checks that the transactions that were not returned are still in the mempool.
    assert_eq_mempool_queue(&mempool, remaining_txs);
}

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    let input_tip_50_address_0 = add_tx_input!(tip: 50, tx_hash: 1);
    let input_tip_100_address_1 = add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1");
    let input_tip_80_address_2 = add_tx_input!(tip: 80, tx_hash: 3, sender_address: "0x2");

    add_tx(&mut mempool, &input_tip_50_address_0);
    add_tx(&mut mempool, &input_tip_100_address_1);
    add_tx(&mut mempool, &input_tip_80_address_2);

    let expected_queue =
        &[input_tip_100_address_1.tx, input_tip_80_address_2.tx, input_tip_50_address_0.tx];
    assert_eq_mempool_queue(&mempool, expected_queue)
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
    let input1 = add_tx_input!(tip: 1, tx_hash: 2);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(tip: 1, tx_hash: 1, sender_address: "0x1");

    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    assert_eq_mempool_queue(&mempool, &[input1.tx, input2.tx]);
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
