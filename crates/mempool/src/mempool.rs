use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{
    Account, AccountState, MempoolInput, MempoolResult, ThinTransaction,
};

use crate::priority_queue::TransactionPriorityQueue;
use crate::transaction_pool::TransactionPool;

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

#[derive(Debug)]
pub struct Mempool {
    // TODO: add docstring explaining visibility and coupling of the fields.
    txs_queue: TransactionPriorityQueue,
    tx_pool: TransactionPool,
    state: HashMap<ContractAddress, AccountState>,
}

impl Mempool {
    pub fn new(inputs: impl IntoIterator<Item = MempoolInput>) -> Self {
        let mut mempool = Mempool {
            txs_queue: TransactionPriorityQueue::default(),
            tx_pool: TransactionPool::default(),
            state: HashMap::default(),
        };

        for MempoolInput { tx, account: Account { sender_address, state } } in inputs.into_iter() {
            // Attempts to insert a key-value pair into the mempool's state. Returns `None`
            // if the key was not present, otherwise returns the old value while updating
            // the new value.
            if mempool.state.insert(sender_address, state).is_some() {
                panic!(
                    "Sender address: {:?} already exists in the mempool. Can't add {:?} to the \
                     mempool.",
                    sender_address, tx
                );
            }
            // Attempt to push the transaction into the tx_pool
            if let Err(err) = mempool.tx_pool.push(tx.clone()) {
                panic!(
                    "Transaction: {:?} already exists in the mempool. Error: {:?}",
                    tx.tx_hash, err
                );
            }

            mempool.txs_queue.push(tx);
        }

        mempool
    }

    pub fn empty() -> Self {
        Mempool::new([])
    }

    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until `commit_block` is invoked.
    // TODO: the last part about commit_block is incorrect if we delete txs in get_txs and then push
    // back. TODO: Consider renaming to `pop_txs` to be more consistent with the standard
    // library.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<ThinTransaction>> {
        let txs = self.txs_queue.pop_last_chunk(n_txs);
        for tx in &txs {
            self.state.remove(&tx.sender_address);
            self.tx_pool.remove(tx.tx_hash)?;
        }

        Ok(txs)
    }

    /// Adds a new transaction to the mempool.
    /// TODO: support fee escalation and transactions with future nonces.
    /// TODO: change input type to `MempoolInput`.
    pub fn add_tx(&mut self, tx: ThinTransaction, account: Account) -> MempoolResult<()> {
        match self.state.entry(account.sender_address) {
            Occupied(_) => Err(MempoolError::DuplicateTransaction { tx_hash: tx.tx_hash }),
            Vacant(entry) => {
                entry.insert(account.state);
                // TODO(Mohammad): use `handle_tx`.
                self.txs_queue.push(tx.clone());
                self.tx_pool.push(tx)?;

                Ok(())
            }
        }
    }

    /// Update the mempool's internal state according to the committed block's transactions.
    /// This method also updates internal state (resolves nonce gaps, updates account balances).
    // TODO: the part about resolving nonce gaps is incorrect if we delete txs in get_txs and then
    // push back.
    pub fn commit_block(
        &mut self,
        _block_number: u64,
        _txs_in_block: &[TransactionHash],
        _state_changes: HashMap<ContractAddress, AccountState>,
    ) -> MempoolResult<()> {
        todo!()
    }
}
