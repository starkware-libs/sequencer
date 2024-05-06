use crate::{errors::MempoolError, priority_queue::PriorityQueue};
use starknet_api::{
    core::{ContractAddress, Nonce},
    internal_transaction::InternalTransaction,
    transaction::TransactionHash,
};
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

pub type MempoolResult<T> = Result<T, MempoolError>;

#[derive(Default)]
pub struct Mempool {
    // TODO: add docstring explaining visibility and coupling of the fields.
    txs_queue: PriorityQueue,
    state: HashMap<ContractAddress, AccountState>,
}

impl Mempool {
    pub fn new(inputs: impl IntoIterator<Item = MempoolInput>) -> Self {
        let mut mempool = Mempool::default();

        mempool.txs_queue = PriorityQueue::from_iter(inputs.into_iter().map(|input| {
            let prev_value = mempool.state.insert(
                input.account.address, input.account.state
            );
            // Assert that the contract address does not exist in the mempool's state to ensure that
            // there is only one transaction per contract address.
            assert!(
                prev_value.is_none(),
                "Contract address: {:?} already exists in the mempool. Can't add {:?} to the mempool.",
                input.account.address, input.tx
            );
            input.tx
        }));

        mempool
    }

    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until `commit_block` is invoked.
    // TODO: the last part about commit_block is incorrect if we delete txs in get_txs and then push back.
    // TODO: Consider renaming to `pop_txs` to be more consistent with the standard library.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalTransaction>> {
        let txs = self.txs_queue.pop_last_chunk(n_txs);
        for tx in &txs {
            self.state.remove(&tx.contract_address());
        }

        Ok(txs)
    }

    /// Adds a new transaction to the mempool.
    /// TODO: support fee escalation and transactions with future nonces.
    pub fn add_tx(&mut self, tx: InternalTransaction, account: Account) -> MempoolResult<()> {
        match self.state.entry(account.address) {
            Occupied(_) => Err(MempoolError::DuplicateTransaction {
                tx_hash: tx.tx_hash(),
            }),
            Vacant(entry) => {
                entry.insert(account.state);
                self.txs_queue.push(tx);
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

#[derive(Clone, Copy, Debug, Default)]
pub struct AccountState {
    pub nonce: Nonce,
    // TODO: add balance field when needed.
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Account {
    pub address: ContractAddress,
    pub state: AccountState,
}

#[derive(Debug)]
pub struct MempoolInput {
    pub tx: InternalTransaction,
    pub account: Account,
}
