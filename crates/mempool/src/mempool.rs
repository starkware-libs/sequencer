use std::collections::HashMap;

use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{
    AccountState,
    AddTransactionArgs,
    CommitBlockArgs,
    MempoolResult,
};
use tracing;

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
    // TODO(Elin): make configurable; should be bounded?
    // Percentage increase for tip and max gas price to enable transaction replacement.
    fee_escalation_percentage: u8, // E.g., 10 for a 10% increase.
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
    // TODO: Consider renaming to `pop_txs` to be more consistent with the standard library.
    #[tracing::instrument(skip(self), err)]
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<Transaction>> {
        let mut eligible_tx_references: Vec<TransactionReference> = Vec::with_capacity(n_txs);
        let mut n_remaining_txs = n_txs;

        while n_remaining_txs > 0 && !self.tx_queue.has_ready_txs() {
            let chunk = self.tx_queue.pop_ready_chunk(n_remaining_txs);
            self.enqueue_next_eligible_txs(&chunk)?;
            n_remaining_txs -= chunk.len();
            eligible_tx_references.extend(chunk);
        }

        // Update the mempool state with the given transactions' nonces.
        for tx_ref in &eligible_tx_references {
            self.mempool_state.insert(tx_ref.address, tx_ref.nonce);
        }

        tracing::info!(
            "Returned {} out of {n_txs} transactions, ready for sequencing.",
            eligible_tx_references.len()
        );

        Ok(eligible_tx_references
            .iter()
            .map(|tx_ref| {
                self.tx_pool
                    .get_by_tx_hash(tx_ref.tx_hash)
                    .expect("Transaction hash from queue must appear in pool.")
            })
            .cloned() // Soft-delete: return without deleting from mempool.
            .collect())
    }

    /// Adds a new transaction to the mempool.
    #[tracing::instrument(
        skip(self, args),
        fields( // Log subset of (informative) fields.
            tx_nonce = %args.tx.nonce(),
            tx_hash = %args.tx.tx_hash(),
            account_state = %args.account_state
        ),
        ret,
        err
    )]
    pub fn add_tx(&mut self, args: AddTransactionArgs) -> MempoolResult<()> {
        let AddTransactionArgs { tx, account_state } = args;
        self.validate_incoming_nonce(tx.nonce(), account_state)?;

        self.handle_fee_escalation(&tx)?;
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
    #[tracing::instrument(skip(self, args), ret, err)]
    pub fn commit_block(&mut self, args: CommitBlockArgs) -> MempoolResult<()> {
        let CommitBlockArgs { nonces, tx_hashes } = args;
        tracing::debug!("Committing block with {} transactions to mempool.", tx_hashes.len());

        // Align mempool data to committed nonces.
        for (&address, &nonce) in &nonces {
            let next_nonce = nonce.try_increment().map_err(|_| MempoolError::FeltOutOfRange)?;
            let account_state = AccountState { address, nonce: next_nonce };
            self.align_to_account_state(account_state);
        }
        tracing::debug!("Aligned mempool to committed nonces.");

        // Rewind nonces of addresses that were not included in block.
        let known_addresses_not_included_in_block =
            self.mempool_state.keys().filter(|&key| !nonces.contains_key(key));
        for address in known_addresses_not_included_in_block {
            // Account nonce is the minimal nonce of this address: it was proposed but not included.
            let tx_reference = self
                .tx_pool
                .account_txs_sorted_by_nonce(*address)
                .next()
                .expect("Address {address} should appear in transaction pool.");
            self.tx_queue.insert(*tx_reference);
        }

        // Hard-delete: finally, remove committed transactions from the mempool.
        for tx_hash in tx_hashes {
            let Ok(_tx) = self.tx_pool.remove(tx_hash) else {
                continue; // Transaction hash unknown to mempool, from a different node.
            };

            // TODO(clean_account_nonces): remove address from nonce table after a block cycle /
            // TTL.
        }
        tracing::debug!("Removed committed transactions known to mempool.");

        // Commit: clear block creation staged state.
        self.mempool_state.clear();

        tracing::debug!("Successfully committed block to mempool.");

        Ok(())
    }

    // TODO(Mohammad): Rename this method once consensus API is added.
    fn _update_gas_price_threshold(&mut self, threshold: GasPrice) {
        self.tx_queue._update_gas_price_threshold(threshold);
    }

    fn validate_incoming_nonce(
        &self,
        tx_nonce: Nonce,
        account_state: AccountState,
    ) -> MempoolResult<()> {
        let AccountState { address, nonce: account_nonce } = account_state;
        let duplicate_nonce_error = MempoolError::DuplicateNonce { address, nonce: tx_nonce };

        // Stateless checks.

        // Check the input: transaction nonce against given account state.
        if account_nonce > tx_nonce {
            return Err(duplicate_nonce_error);
        }

        // Stateful checks.

        // Check nonce against mempool state.
        if let Some(mempool_state_nonce) = self.mempool_state.get(&address) {
            if mempool_state_nonce >= &tx_nonce {
                return Err(duplicate_nonce_error);
            }
        }

        // Check nonce against the queue.
        if self.tx_queue.get_nonce(address).is_some_and(|queued_nonce| queued_nonce >= tx_nonce) {
            return Err(duplicate_nonce_error);
        }

        Ok(())
    }

    fn enqueue_next_eligible_txs(&mut self, txs: &[TransactionReference]) -> MempoolResult<()> {
        for tx in txs {
            let current_account_state = AccountState { address: tx.address, nonce: tx.nonce };

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

    fn handle_fee_escalation(&mut self, incoming_tx: &Transaction) -> MempoolResult<()> {
        let incoming_tx_ref = TransactionReference::new(incoming_tx);
        let TransactionReference { address, nonce, .. } = incoming_tx_ref;
        let Some(existing_tx_ref) = self.tx_pool.get_by_address_and_nonce(address, nonce) else {
            // Replacement irrelevant: no existing transaction with the same nonce for address.
            return Ok(());
        };

        if !self.should_replace_tx(existing_tx_ref, &incoming_tx_ref) {
            tracing::info!(
                "{existing_tx_ref} was not replaced by {incoming_tx_ref} due to insufficient
                fee escalation."
            );
            // TODO(Elin): consider adding a more specific error type / message.
            return Err(MempoolError::DuplicateNonce { address, nonce });
        }

        tracing::info!("{existing_tx_ref} will be replaced by {incoming_tx_ref}.");

        self.tx_queue.remove(address);
        self.tx_pool
            .remove(existing_tx_ref.tx_hash)
            .expect("Transaction hash from pool must exist.");

        Ok(())
    }

    fn should_replace_tx(
        &self,
        existing_tx: &TransactionReference,
        incoming_tx: &TransactionReference,
    ) -> bool {
        let [existing_tip, incoming_tip] =
            [existing_tx, incoming_tx].map(|tx| u128::from(tx.tip.0));
        let [existing_max_l2_gas_price, incoming_max_l2_gas_price] =
            [existing_tx, incoming_tx].map(|tx| tx.max_l2_gas_price.0);

        self.increased_enough(existing_tip, incoming_tip)
            && self.increased_enough(existing_max_l2_gas_price, incoming_max_l2_gas_price)
    }

    fn increased_enough(&self, existing_value: u128, incoming_value: u128) -> bool {
        // E.g., 110 for a 10% increase.
        let escalation_factor = 100 + u128::from(self.fee_escalation_percentage);

        // TODO(Elin): add overflow tests; 2^127 existing with 10% increase should not overflow.
        let Some(escalation_qualified_value) =
            existing_value.checked_mul(escalation_factor).map(|v| v / 100)
        else {
            // Overflow occurred; cannot calculate required increase. Reject the transaction.
            return false;
        };

        incoming_value >= escalation_qualified_value
    }
}

/// Provides a lightweight representation of a transaction for mempool usage (e.g., excluding
/// execution fields).
/// TODO(Mohammad): rename this struct to `ThinTransaction` once that name
/// becomes available, to better reflect its purpose and usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionReference {
    pub address: ContractAddress,
    pub nonce: Nonce,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
    pub max_l2_gas_price: GasPrice,
}

impl std::fmt::Display for TransactionReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let TransactionReference { address, nonce, tx_hash, tip, max_l2_gas_price } = self;
        write!(
            f,
            "TransactionReference {{ address: {address}, nonce: {nonce}, tx_hash: {tx_hash},
            tip: {tip}, max_l2_gas_price: {max_l2_gas_price} }}"
        )
    }
}

impl TransactionReference {
    pub fn new(tx: &Transaction) -> Self {
        TransactionReference {
            address: tx.contract_address(),
            nonce: tx.nonce(),
            tx_hash: tx.tx_hash(),
            tip: tx.tip().expect("Expected a valid tip value."),
            max_l2_gas_price: tx
                .resource_bounds()
                .expect("Expected a valid resource bounds value.")
                .get_l2_bounds()
                .max_price_per_unit,
        }
    }
}
