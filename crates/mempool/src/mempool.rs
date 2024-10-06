use std::collections::HashMap;

use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{
    AccountState,
    AddTransactionArgs,
    CommitBlockArgs,
    MempoolResult,
};

use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

type AccountToNonce = HashMap<ContractAddress, Nonce>;

#[derive(Debug, Default)]
pub struct Mempool {
    // TODO: add docstring explaining visibility and coupling of the fields.
    // All transactions currently held in the mempool.
    tx_pool: TransactionPool,
    // Transactions eligible for sequencing.
    tx_queue: TransactionQueue,
    // Represents the state of the mempool during block creation.
    mempool_state: HashMap<ContractAddress, Nonce>,
    // The most recent account nonces received, for all account in the pool.
    account_nonces: AccountToNonce,
}

impl Mempool {
    pub fn empty() -> Self {
        Mempool::default()
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter(&self) -> impl Iterator<Item = &TransactionReference> {
        self.tx_queue.iter_over_ready_txs()
    }

    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until the block in-progress is
    /// created.
    // TODO: the last part about commit_block is incorrect if we delete txs in get_txs and then push
    // back. TODO: Consider renaming to `pop_txs` to be more consistent with the standard
    // library.
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<Transaction>> {
        let mut eligible_tx_references: Vec<TransactionReference> = Vec::with_capacity(n_txs);
        let mut n_remaining_txs = n_txs;

        while n_remaining_txs > 0 && !self.tx_queue.has_ready_txs() {
            let chunk = self.tx_queue.pop_ready_chunk(n_remaining_txs);
            self.enqueue_next_eligible_txs(&chunk)?;
            n_remaining_txs -= chunk.len();
            eligible_tx_references.extend(chunk);
        }

        let mut eligible_txs: Vec<Transaction> = Vec::with_capacity(n_txs);
        for tx_ref in &eligible_tx_references {
            let tx = self.tx_pool.remove(tx_ref.tx_hash)?;
            // TODO(clean_account_nonces): remove address from nonce table after a block cycle /
            // TTL.
            eligible_txs.push(tx);
        }

        // Update the mempool state with the given transactions' nonces.
        for tx_ref in &eligible_tx_references {
            self.mempool_state.insert(tx_ref.sender_address, tx_ref.nonce);
        }

        Ok(eligible_txs)
    }

    /// Adds a new transaction to the mempool.
    /// TODO: support fee escalation and transactions with future nonces.
    /// TODO: check Account nonce and balance.
    pub fn add_tx(&mut self, args: AddTransactionArgs) -> MempoolResult<()> {
        self.validate_input(&args)?;

        let AddTransactionArgs { tx, account_state } = args;
        self.tx_pool.insert(tx)?;

        // Align to account nonce, only if it is at least the one stored.
        let AccountState { address, nonce } = account_state;
        match self.account_nonces.get(&address) {
            Some(stored_account_nonce) if &nonce < stored_account_nonce => {}
            _ => {
                self.align_to_account_state(account_state);
            }
        }

        Ok(())
    }

    /// Update the mempool's internal state according to the committed block (resolves nonce gaps,
    /// updates account balances).
    // TODO: the part about resolving nonce gaps is incorrect if we delete txs in get_txs and then
    // push back.
    pub fn commit_block(&mut self, args: CommitBlockArgs) -> MempoolResult<()> {
        for (&address, &nonce) in &args.nonces {
            let next_nonce = nonce.try_increment().map_err(|_| MempoolError::FeltOutOfRange)?;
            let account_state = AccountState { address, nonce: next_nonce };
            self.align_to_account_state(account_state);
        }

        // Rewind nonces of addresses that were not included in block.
        let addresses_not_included_in_block =
            self.mempool_state.keys().filter(|&key| !args.nonces.contains_key(key));
        for address in addresses_not_included_in_block {
            self.tx_queue.remove(*address);
        }

        // Commit: clear block creation staged state.
        self.mempool_state.clear();

        Ok(())
    }

    // TODO(Mohammad): Rename this method once consensus API is added.
    fn _update_gas_price_threshold(&mut self, threshold: u128) {
        self.tx_queue._update_gas_price_threshold(threshold);
    }

    fn validate_input(&self, args: &AddTransactionArgs) -> MempoolResult<()> {
        let sender_address = args.tx.contract_address();
        let tx_nonce = args.tx.nonce();
        let duplicate_nonce_error =
            MempoolError::DuplicateNonce { address: sender_address, nonce: tx_nonce };

        // Stateless checks.

        // Check the input: transaction nonce against given account state.
        let account_nonce = args.account_state.nonce;
        if account_nonce > tx_nonce {
            return Err(duplicate_nonce_error);
        }

        // Stateful checks.

        // Check nonce against mempool state.
        if let Some(mempool_state_nonce) = self.mempool_state.get(&sender_address) {
            if mempool_state_nonce >= &tx_nonce {
                return Err(duplicate_nonce_error);
            }
        }

        // Check nonce against the queue.
        if self
            .tx_queue
            .get_nonce(sender_address)
            .is_some_and(|queued_nonce| queued_nonce >= tx_nonce)
        {
            return Err(duplicate_nonce_error);
        }

        Ok(())
    }

    fn enqueue_next_eligible_txs(&mut self, txs: &[TransactionReference]) -> MempoolResult<()> {
        for tx in txs {
            let current_account_state =
                AccountState { address: tx.sender_address, nonce: tx.nonce };

            if let Some(next_tx_reference) =
                self.tx_pool.get_next_eligible_tx(current_account_state)?
            {
                self.tx_queue.insert(*next_tx_reference);
            }
        }

        Ok(())
    }

    fn align_to_account_state(&mut self, account_state: AccountState) {
        let AccountState { address, nonce } = account_state;
        // Maybe remove out-of-date transactions.
        // Note: != is equivalent to > in `add_tx`, as lower nonces are rejected in validation.
        if self.tx_queue.get_nonce(address).is_some_and(|queued_nonce| queued_nonce != nonce) {
            assert!(self.tx_queue.remove(address), "Expected to remove address from queue.");
        }

        // Remove from pool.
        self.tx_pool.remove_up_to_nonce(address, nonce);
        // TODO(clean_account_nonces): remove address from nonce table after a block cycle / TTL.
        self.account_nonces.insert(address, nonce);

        // Maybe close nonce gap.
        if self.tx_queue.get_nonce(address).is_none() {
            if let Some(tx_reference) = self.tx_pool.get_by_address_and_nonce(address, nonce) {
                self.tx_queue.insert(*tx_reference);
            }
        }
    }
}

/// Provides a lightweight representation of a transaction for mempool usage (e.g., excluding
/// execution fields).
/// TODO(Mohammad): rename this struct to `ThinTransaction` once that name
/// becomes available, to better reflect its purpose and usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionReference {
    pub sender_address: ContractAddress,
    pub nonce: Nonce,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
    pub resource_bounds: ValidResourceBounds,
}

impl TransactionReference {
    pub fn new(tx: &Transaction) -> Self {
        TransactionReference {
            sender_address: tx.contract_address(),
            nonce: tx.nonce(),
            tx_hash: tx.tx_hash(),
            tip: tx.tip().expect("Expected a valid tip value."),
            resource_bounds: *tx
                .resource_bounds()
                .expect("Expected a valid resource bounds value."),
        }
    }

    pub fn get_l2_gas_price(&self) -> u128 {
        self.resource_bounds.get_l2_bounds().max_price_per_unit.0
    }
}
