use std::collections::{btree_map, hash_map, BTreeMap, HashMap, VecDeque};

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{MempoolResult, ThinTransaction};

use crate::mempool::TransactionReference;

/// Contains all transactions currently held in the mempool.
/// Invariant: both data structures are consistent regarding the existence of transactions:
/// A transaction appears in one if and only if it appears in the other.
/// No duplicate transactions appear in the pool.
#[derive(Debug, Default)]
pub struct TransactionPool {
    // Holds the complete transaction objects; it should be the sole entity that does so.
    tx_pool: HashMap<TransactionHash, ThinTransaction>,
    // Transactions organized by account address, sorted by ascending nonce values.
    txs_by_account: HashMap<ContractAddress, BTreeMap<Nonce, TransactionReference>>,
}

impl TransactionPool {
    // TODO(Mohammad): Remove the cloning of tx once the `TransactionReference` is updated.
    pub fn insert(&mut self, tx: ThinTransaction) -> MempoolResult<()> {
        let tx_hash = tx.tx_hash;

        // Insert transaction to pool, if it is new.
        if let hash_map::Entry::Vacant(entry) = self.tx_pool.entry(tx_hash) {
            entry.insert(tx.clone());
        } else {
            return Err(MempoolError::DuplicateTransaction { tx_hash });
        }

        let txs_from_account_entry = self.txs_by_account.entry(tx.sender_address).or_default();
        match txs_from_account_entry.entry(tx.nonce) {
            btree_map::Entry::Vacant(txs_from_account) => {
                txs_from_account.insert(TransactionReference::new(tx));
            }
            btree_map::Entry::Occupied(_) => {
                panic!(
                    "Transaction pool consistency error: transaction with hash {tx_hash} does not \
                     appear in main mapping, but it appears in the account mapping"
                );
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, tx_hash: TransactionHash) -> MempoolResult<ThinTransaction> {
        let tx =
            self.tx_pool.remove(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })?;

        let error_message = |tx_hash| {
            format!(
                "Transaction pool consistency error: transaction with hash {tx_hash} appears in \
                 main mapping, but does not appear in the account mapping"
            )
        };

        let txs_from_account_entry = self.txs_by_account.entry(tx.sender_address);
        match txs_from_account_entry {
            hash_map::Entry::Occupied(mut entry) => {
                let txs_from_account = entry.get_mut();
                assert!(txs_from_account.remove(&tx.nonce).is_some(), "{}", error_message(tx_hash));
            }
            hash_map::Entry::Vacant(_) => panic!("{}", error_message(tx_hash)),
        }
        Ok(tx)
    }

    pub fn get(&self, tx_hash: TransactionHash) -> MempoolResult<&ThinTransaction> {
        self.tx_pool.get(&tx_hash).ok_or(MempoolError::TransactionNotFound { tx_hash })
    }
}

// TODO: Use in txs_by_account.
// TODO: remove when is used.
#[allow(dead_code)]
// Invariant: Transactions have strictly increasing nonces, without gaps.
// Assumption: Transactions are provided in the correct order.
#[derive(Default)]
pub struct AccountTransactionIndex(VecDeque<ThinTransaction>);

// TODO: remove when is used.
#[allow(dead_code)]
impl AccountTransactionIndex {
    pub fn push(&mut self, tx: ThinTransaction) {
        if let Some(last_tx) = self.0.back() {
            assert_eq!(
                tx.nonce,
                last_tx.nonce.try_increment().expect("Nonce overflow."),
                "Nonces must be strictly increasing without gaps."
            );
        }

        self.0.push_back(tx);
    }

    pub fn top(&self) -> Option<&ThinTransaction> {
        self.0.front()
    }

    pub fn pop_front(&mut self) -> Option<ThinTransaction> {
        self.0.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, tx: &ThinTransaction) -> bool {
        self.0.contains(tx)
    }
}
