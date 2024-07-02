use assert_matches::assert_matches;
use itertools::zip_eq;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_api::{contract_address, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{Account, ThinTransaction};

use crate::mempool::{Mempool, MempoolInput, TransactionReference};

/// Creates a valid input for mempool's `add_tx` with optional default values.
/// Usage:
/// 1. add_tx_input!(tip: 1, tx_hash: StarkFelt::TWO, address: "0x3", nonce: 4)
/// 2. add_tx_input!(tip: 1, tx_hash: StarkFelt::TWO, address: "0x3")
/// 3. add_tx_input!(tip:1 , tx_hash: StarkFelt::TWO)
macro_rules! add_tx_input {
    // Pattern for all four arguments with keyword arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, nonce: $nonce:expr) => {{
        let sender_address = contract_address!($sender_address);
        let account = Account { sender_address, ..Default::default() };
        let tx = ThinTransaction {
            tip: Tip($tip),
            tx_hash: TransactionHash($tx_hash),
            sender_address,
            nonce: Nonce::from($nonce),
        };
        MempoolInput { tx, account }
    }};
    // Pattern for three arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, nonce: Nonce::default())
    };
    // Pattern for two arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: ContractAddress::default(), nonce: Nonce::default())
    };
}

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

// Asserts that the transactions in the mempool are in ascending order as per the expected
// transactions.
#[track_caller]
fn check_mempool_txs_eq(mempool: &Mempool, expected_txs: &[ThinTransaction]) {
    let mempool_txs = mempool.tx_queue.iter();
    let expected_txs = expected_txs.iter().map(TransactionReference::new);

    assert!(
        zip_eq(expected_txs, mempool_txs)
            // Deref the inner mempool tx type.
            .all(|(expected_tx, mempool_tx)| expected_tx == *mempool_tx)
    );
}

#[rstest]
#[case(3)] // Requesting exactly the number of transactions in the queue
#[case(5)] // Requesting more transactions than are in the queue
#[case(2)] // Requesting fewer transactions than are in the queue
fn test_get_txs(#[case] requested_txs: usize) {
    let input_tip_50_address_0 = add_tx_input!(tip: 50, tx_hash: StarkFelt::ONE);
    let input_tip_100_address_1 =
        add_tx_input!(tip: 100, tx_hash: StarkFelt::TWO, sender_address: "0x1");
    let input_tip_10_address_2 =
        add_tx_input!(tip: 10, tx_hash: StarkFelt::THREE, sender_address: "0x2");

    let mut mempool = Mempool::new([
        input_tip_50_address_0.clone(),
        input_tip_100_address_1.clone(),
        input_tip_10_address_2.clone(),
    ])
    .unwrap();

    let sorted_txs =
        [input_tip_100_address_1.tx, input_tip_50_address_0.tx, input_tip_10_address_2.tx];

    let txs = mempool.get_txs(requested_txs).unwrap();

    // This ensures we do not exceed the priority queue's limit of 3 transactions.
    let max_requested_txs = requested_txs.min(3);

    // checks that the returned transactions are the ones with the highest priority.
    let (expected_txs, remaining_txs) = sorted_txs.split_at(max_requested_txs);
    assert_eq!(txs, expected_txs);

    // checks that the transactions that were not returned are still in the mempool.
    check_mempool_txs_eq(&mempool, remaining_txs);
}

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    let input_tip_50_address_0 = add_tx_input!(tip: 50, tx_hash: StarkFelt::ONE);
    let input_tip_100_address_1 =
        add_tx_input!(tip: 100, tx_hash: StarkFelt::TWO, sender_address: "0x1");
    let input_tip_80_address_2 =
        add_tx_input!(tip: 80, tx_hash: StarkFelt::THREE, sender_address: "0x2");

    assert_eq!(mempool.add_tx(input_tip_50_address_0.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input_tip_100_address_1.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input_tip_80_address_2.clone()), Ok(()));

    let expected_txs =
        &[input_tip_50_address_0.tx, input_tip_80_address_2.tx, input_tip_100_address_1.tx];
    check_mempool_txs_eq(&mempool, expected_txs)
}

#[test]
fn test_new_with_duplicate_tx() {
    let input = add_tx_input!(tip: 0, tx_hash: StarkFelt::ONE);
    let same_input = input.clone();

    assert!(matches!(
        Mempool::new([input, same_input]),
        Err(MempoolError::DuplicateTransaction { tx_hash: TransactionHash(StarkFelt::ONE) })
    ));
}

#[rstest]
fn test_add_tx_with_duplicate_tx(mut mempool: Mempool) {
    let input = add_tx_input!(tip: 50, tx_hash: StarkFelt::ONE);
    let same_input = input.clone();

    assert_eq!(mempool.add_tx(input), Ok(()));

    assert_matches!(
        mempool.add_tx(same_input.clone()),
        Err(MempoolError::DuplicateTransaction { .. })
    );
    // Assert that the original tx remains in the pool after the failed attempt.
    check_mempool_txs_eq(&mempool, &[same_input.tx])
}

#[rstest]
fn test_add_tx_with_identical_tip_succeeds(mut mempool: Mempool) {
    let input1 = add_tx_input!(tip: 1, tx_hash: StarkFelt::TWO);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(tip: 1, tx_hash: StarkFelt::ONE, sender_address: "0x1");

    assert_eq!(mempool.add_tx(input1.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input2.clone()), Ok(()));

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    check_mempool_txs_eq(&mempool, &[input2.tx, input1.tx]);
}

#[rstest]
fn test_tip_priority_over_tx_hash(mut mempool: Mempool) {
    let input_big_tip_small_hash = add_tx_input!(tip: 2, tx_hash: StarkFelt::ONE);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash =
        add_tx_input!(tip: 1, tx_hash: StarkFelt::TWO, sender_address: "0x1");

    assert_eq!(mempool.add_tx(input_big_tip_small_hash.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input_small_tip_big_hash.clone()), Ok(()));
    check_mempool_txs_eq(&mempool, &[input_small_tip_big_hash.tx, input_big_tip_small_hash.tx])
}
