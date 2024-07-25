use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{
    Account,
    AccountState,
    MempoolInput,
    MempoolResult,
    ThinTransaction,
};

use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

#[derive(Debug, Default)]
pub struct Mempool {
    // TODO: add docstring explaining visibility and coupling of the fields.
    // All transactions currently held in the mempool.
    tx_pool: TransactionPool,
    // Transactions eligible for sequencing.
    tx_queue: TransactionQueue,
}

impl Mempool {
    pub fn new(inputs: impl IntoIterator<Item = MempoolInput>) -> MempoolResult<Self> {
        let mut mempool = Mempool::empty();

        for input in inputs {
            mempool.insert_tx(input)?;
        }
        Ok(mempool)
    }

    pub fn empty() -> Self {
        Mempool::default()
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter(&self) -> impl Iterator<Item = &TransactionReference> {
        self.tx_queue.iter()
    }

    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until `commit_block` is invoked.
    // TODO: the last part about commit_block is incorrect if we delete txs in get_txs and then push
    // back. TODO: Consider renaming to `pop_txs` to be more consistent with the standard
    // library.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<ThinTransaction>> {
        let mut eligible_txs: Vec<ThinTransaction> = Vec::with_capacity(n_txs);
        for tx_hash in self.tx_queue.pop_chunk(n_txs) {
            let tx = self.tx_pool.remove(tx_hash)?;
            eligible_txs.push(tx);
        }

        Ok(eligible_txs)
    }

    /// Adds a new transaction to the mempool.
    /// TODO: support fee escalation and transactions with future nonces.
    /// TODO: check Account nonce and balance.
    pub fn add_tx(&mut self, input: MempoolInput) -> MempoolResult<()> {
        self.insert_tx(input)
    }

    /// Update the mempool's internal state according to the committed block (resolves nonce gaps,
    /// updates account balances).
    // TODO: the part about resolving nonce gaps is incorrect if we delete txs in get_txs and then
    // push back.
    // state_changes: a map that associates each account address with the state of the committed
    // block.
    pub fn commit_block(
        &mut self,
        state_changes: HashMap<ContractAddress, AccountState>,
    ) -> MempoolResult<()> {
        for (address, AccountState { nonce }) in state_changes {
            let next_nonce = nonce.try_increment().map_err(|_| MempoolError::FeltOutOfRange)?;

            // Align the queue with the committed nonces.
            if self
                .tx_queue
                .get_nonce(address)
                .is_some_and(|queued_nonce| queued_nonce != next_nonce)
            {
                self.tx_queue.remove(address);
            }

            if self.tx_queue.get_nonce(address).is_none() {
                if let Some(tx) = self.tx_pool.get_by_address_and_nonce(address, nonce) {
                    self.tx_queue.insert(*tx);
                }
            }

            self.tx_pool.remove_up_to_nonce(address, next_nonce);
        }
        Ok(())
    }

    fn insert_tx(&mut self, input: MempoolInput) -> MempoolResult<()> {
        let MempoolInput { tx, account: Account { sender_address, state: AccountState { nonce } } } =
            input;

        self.tx_pool.insert(tx)?;

        // Maybe close nonce gap.
        if self.tx_queue.get_nonce(sender_address).is_none() {
            if let Some(tx_reference) = self.tx_pool.get_by_address_and_nonce(sender_address, nonce)
            {
                self.tx_queue.insert(*tx_reference);
            }
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn _tx_pool(&self) -> &TransactionPool {
        &self.tx_pool
    }
}

/// Provides a lightweight representation of a transaction for mempool usage (e.g., excluding
/// execution fields).
/// TODO(Mohammad): rename this struct to `ThinTransaction` once that name
/// becomes available, to better reflect its purpose and usage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransactionReference {
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
}

impl TransactionReference {
    pub fn new(tx: &ThinTransaction) -> Self {
        TransactionReference {
            sender_address: tx.sender_address,
            nonce: tx.nonce,
            tx_hash: tx.tx_hash,
            tip: tx.tip,
        }
    }
}
