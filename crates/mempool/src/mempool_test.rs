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

/// Creates a valid input for mempool's `add_tx` with optional default value for
/// `sender_address`.
/// Usage:
/// 1. add_tx_input!(tip, tx_hash, address, nonce)
/// 2. add_tx_input!(tip, tx_hash, address)
/// 3. add_tx_input!(tip, tx_hash)
macro_rules! add_tx_input {
    // Pattern for all four arguments
    ($tip:expr, $tx_hash:expr, $sender_address:expr, $nonce:expr) => {{
        let account = Account { sender_address: $sender_address, ..Default::default() };
        let tx = ThinTransaction {
            tip: $tip,
            tx_hash: $tx_hash,
            sender_address: $sender_address,
            nonce: $nonce,
        };
        MempoolInput { tx, account }
    }};
    // Pattern for three arguments: tip, tx_hash, address
    ($tip:expr, $tx_hash:expr, $address:expr) => {
        add_tx_input!($tip, $tx_hash, $address, Nonce::default())
    };
    // Pattern for two arguments: tip, tx_hash
    ($tip:expr, $tx_hash:expr) => {
        add_tx_input!($tip, $tx_hash, ContractAddress::default(), Nonce::default())
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
    let input_tip_50_address_0 = add_tx_input!(Tip(50), TransactionHash(StarkFelt::ONE));
    let input_tip_100_address_1 =
        add_tx_input!(Tip(100), TransactionHash(StarkFelt::TWO), contract_address!("0x1"));
    let input_tip_10_address_2 =
        add_tx_input!(Tip(10), TransactionHash(StarkFelt::THREE), contract_address!("0x2"));

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
    let input_tip_50_address_0 = add_tx_input!(Tip(50), TransactionHash(StarkFelt::ONE));
    let input_tip_100_address_1 =
        add_tx_input!(Tip(100), TransactionHash(StarkFelt::TWO), contract_address!("0x1"));
    let input_tip_80_address_2 =
        add_tx_input!(Tip(80), TransactionHash(StarkFelt::THREE), contract_address!("0x2"));

    assert_eq!(mempool.add_tx(input_tip_50_address_0.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input_tip_100_address_1.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input_tip_80_address_2.clone()), Ok(()));

    let expected_txs =
        &[input_tip_50_address_0.tx, input_tip_80_address_2.tx, input_tip_100_address_1.tx];
    check_mempool_txs_eq(&mempool, expected_txs)
}

#[rstest]
fn test_add_same_tx(mut mempool: Mempool) {
    let input = add_tx_input!(Tip(50), TransactionHash(StarkFelt::ONE));
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
    let input1 = add_tx_input!(Tip(1), TransactionHash(StarkFelt::TWO));

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(Tip(1), TransactionHash(StarkFelt::ONE), contract_address!("0x1"));

    assert_eq!(mempool.add_tx(input1.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input2.clone()), Ok(()));

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    check_mempool_txs_eq(&mempool, &[input2.tx, input1.tx]);
}

#[rstest]
fn test_tip_priority_over_tx_hash(mut mempool: Mempool) {
    let input_big_tip_small_hash = add_tx_input!(Tip(2), TransactionHash(StarkFelt::ONE));

    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash =
        add_tx_input!(Tip(1), TransactionHash(StarkFelt::TWO), contract_address!("0x1"));

    assert_eq!(mempool.add_tx(input_big_tip_small_hash.clone()), Ok(()));
    assert_eq!(mempool.add_tx(input_small_tip_big_hash.clone()), Ok(()));
    check_mempool_txs_eq(&mempool, &[input_small_tip_big_hash.tx, input_big_tip_small_hash.tx])
}
