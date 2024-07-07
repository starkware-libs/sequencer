use assert_matches::assert_matches;
use itertools::zip_eq;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_api::{contract_address, felt, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{Account, ThinTransaction};
use starknet_types_core::felt::Felt;

use crate::mempool::{Mempool, MempoolInput, TransactionReference};

#[track_caller]
fn add_tx(mempool: &mut Mempool, input: &MempoolInput) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

/// Creates a valid input for mempool's `add_tx` with optional default values.
/// Usage:
/// 1. add_tx_input!(tip: 1, tx_hash: Felt::TWO, sender_address: 3_u8, nonce: 4)
/// 2. add_tx_input!(tip: 1, tx_hash: Felt::TWO, sender_address: 3_u8)
/// 3. add_tx_input!(tip:1 , tx_hash: Felt::TWO)
macro_rules! add_tx_input {
    // Pattern for all four arguments with keyword arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, nonce: $nonce:expr) => {{
        let sender_address = contract_address!($sender_address);
        let account = Account { sender_address, ..Default::default() };
        let tx = ThinTransaction {
            tip: Tip($tip),
            tx_hash: TransactionHash(StarkHash::from($tx_hash)),
            sender_address,
            nonce: Nonce($nonce),
        };
        MempoolInput { tx, account }
    }};
    // Pattern for three arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, nonce: Felt::ZERO)
    };
    // Pattern for two arguments.
    (tip: $tip:expr, tx_hash: $tx_hash:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: "0x0", nonce: Felt::ZERO)
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
    let input_tip_50_address_0 = add_tx_input!(tip: 50, tx_hash: 1);
    let input_tip_100_address_1 = add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1");
    let input_tip_10_address_2 = add_tx_input!(tip: 10, tx_hash: 3, sender_address: "0x2");

    let txs = [
        input_tip_50_address_0.clone(),
        input_tip_100_address_1.clone(),
        input_tip_10_address_2.clone(),
    ];
    let n_txs = txs.len();

    let mut mempool = Mempool::new(txs).unwrap();

    let sorted_txs =
        [input_tip_100_address_1.tx, input_tip_50_address_0.tx, input_tip_10_address_2.tx];

    let txs = mempool.get_txs(requested_txs).unwrap();

    // This ensures we do not exceed the number of transactions available in the mempool.
    let max_requested_txs = requested_txs.min(n_txs);

    // checks that the returned transactions are the ones with the highest priority.
    let (expected_txs, remaining_txs) = sorted_txs.split_at(max_requested_txs);
    assert_eq!(txs, expected_txs);

    // checks that the transactions that were not returned are still in the mempool.
    check_mempool_txs_eq(&mempool, remaining_txs);
}

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    let input_tip_50_address_0 = add_tx_input!(tip: 50, tx_hash: 1);
    let input_tip_100_address_1 = add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1");
    let input_tip_80_address_2 = add_tx_input!(tip: 80, tx_hash: 3, sender_address: "0x2");

    add_tx(&mut mempool, &input_tip_50_address_0);
    add_tx(&mut mempool, &input_tip_100_address_1);
    add_tx(&mut mempool, &input_tip_80_address_2);

    let expected_txs =
        &[input_tip_100_address_1.tx, input_tip_80_address_2.tx, input_tip_50_address_0.tx];
    check_mempool_txs_eq(&mempool, expected_txs)
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
    check_mempool_txs_eq(&mempool, &[same_input.tx])
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
    check_mempool_txs_eq(&mempool, &[input1.tx, input2.tx]);
}

#[rstest]
fn test_tip_priority_over_tx_hash(mut mempool: Mempool) {
    let input_big_tip_small_hash = add_tx_input!(tip: 2, tx_hash: Felt::ONE);

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash = add_tx_input!(tip: 1, tx_hash: Felt::TWO, sender_address: "0x1");

    add_tx(&mut mempool, &input_big_tip_small_hash);
    add_tx(&mut mempool, &input_small_tip_big_hash);
    check_mempool_txs_eq(&mempool, &[input_big_tip_small_hash.tx, input_small_tip_big_hash.tx])
}
