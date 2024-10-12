use std::collections::{HashMap, HashSet};

use pretty_assertions::assert_eq;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, felt, nonce, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{AddTransactionArgs, CommitBlockArgs};

use crate::mempool::Mempool;

/// Creates an executable invoke transaction with the given field subset (the rest receive default
/// values).
#[macro_export]
macro_rules! tx {
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {{
            let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
                l2_gas: ResourceBounds {
                    max_price_per_unit: GasPrice($max_l2_gas_price),
                    ..Default::default()
                },
                ..Default::default()
            });

            Transaction::Invoke(executable_invoke_tx(invoke_tx_args!{
                sender_address: contract_address!($sender_address),
                tx_hash: TransactionHash(StarkHash::from($tx_hash)),
                tip: Tip($tip),
                nonce: nonce!($tx_nonce),
                resource_bounds,
            }))
    }};
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, max_l2_gas_price: VALID_L2_GAS_MAX_PRICE_PER_UNIT)
    };
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tip: 0, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce)
    };
    (tx_nonce: $tx_nonce:expr, sender_address: $sender_address:expr) => {
        tx!(tip: 0, tx_hash: 0, sender_address: $sender_address, tx_nonce: $tx_nonce)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr, sender_address: $sender_address:expr) => {
        tx!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce)
    };
    (tx_nonce: $tx_nonce:expr) => {
        tx!(tip: 0, tx_hash: 0, sender_address: "0x0", tx_nonce: $tx_nonce)
    };
    (tx_hash: $tx_hash:expr, tip: $tip:expr, max_l2_gas_price: $max_l2_gas_price:expr, tx_nonce: $tx_nonce:expr, sender_address: $sender_address:expr) => {
        tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, max_l2_gas_price: $max_l2_gas_price)
    };
}

/// Creates an input for `add_tx` with the given field subset (the rest receive default values).
#[macro_export]
macro_rules! add_tx_input {
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr, max_l2_gas_price: $max_l2_gas_price:expr) => {{
        let tx = tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, max_l2_gas_price: $max_l2_gas_price);
        let address = contract_address!($sender_address);
        let account_nonce = nonce!($account_nonce);
        let account_state = AccountState { address, nonce: account_nonce};

        AddTransactionArgs { tx, account_state }
    }};
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {{
            add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce, max_l2_gas_price: VALID_L2_GAS_MAX_PRICE_PER_UNIT)
    }};
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0, account_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: 0, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: 1, tx_hash: 0, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: 0, account_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tip: $tip:expr, max_l2_gas_price: $max_l2_gas_price:expr, tx_nonce: $tx_nonce:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: 0,  max_l2_gas_price: $max_l2_gas_price)
    };
}

#[track_caller]
pub fn add_tx(mempool: &mut Mempool, input: &AddTransactionArgs) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

#[track_caller]
pub fn add_tx_expect_error(
    mempool: &mut Mempool,
    input: &AddTransactionArgs,
    expected_error: MempoolError,
) {
    assert_eq!(mempool.add_tx(input.clone()), Err(expected_error));
}

#[track_caller]
pub fn commit_block(
    mempool: &mut Mempool,
    nonces: impl IntoIterator<Item = (&'static str, u8)>,
    tx_hashes: impl IntoIterator<Item = u8>,
) {
    let nonces = HashMap::from_iter(
        nonces.into_iter().map(|(address, nonce)| (contract_address!(address), nonce!(nonce))),
    );
    let tx_hashes =
        HashSet::from_iter(tx_hashes.into_iter().map(|tx_hash| TransactionHash(felt!(tx_hash))));
    let args = CommitBlockArgs { nonces, tx_hashes };

    assert_eq!(mempool.commit_block(args), Ok(()));
}

#[track_caller]
pub fn get_txs_and_assert_expected(
    mempool: &mut Mempool,
    n_txs: usize,
    expected_txs: &[Transaction],
) {
    let txs = mempool.get_txs(n_txs).unwrap();
    assert_eq!(txs, expected_txs);
}
