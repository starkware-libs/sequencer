use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use starknet_api::block::NonzeroGasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::{InternalRpcTransaction, InternalRpcTransactionWithoutTxHash};
use starknet_api::transaction::fields::Tip;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{
    AccountState,
    AddTransactionArgs,
    CommitBlockArgs,
    MempoolResult,
    MempoolSnapshot,
};
use tracing::{debug, info, instrument, trace};

use crate::config::MempoolConfig;
use crate::metrics::{
    metric_count_committed_txs,
    metric_count_expired_txs,
    metric_count_rejected_txs,
    metric_set_get_txs_size,
    MempoolMetricHandle,
};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;
use crate::utils::{try_increment_nonce, Clock};

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

#[cfg(test)]
#[path = "mempool_flow_tests.rs"]
pub mod mempool_flow_tests;

type AddressToNonce = HashMap<ContractAddress, Nonce>;

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
struct CommitHistory {
    commits: VecDeque<AddressToNonce>,
}

impl CommitHistory {
    fn new(capacity: usize) -> Self {
        CommitHistory { commits: std::iter::repeat(AddressToNonce::new()).take(capacity).collect() }
    }

    fn push(&mut self, commit: AddressToNonce) -> AddressToNonce {
        let removed = self.commits.pop_front();
        self.commits.push_back(commit);
        removed.expect("Commit history should be initialized with capacity.")
    }
}

/// Represents the state tracked by the mempool.
/// It is partitioned into categories, each serving a distinct role in the lifecycle of transaction
/// management.
#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub struct MempoolState {
    /// Records recent commit_block events to preserve the committed nonces of the latest blocks.
    commit_history: CommitHistory,
    /// Finalized nonces committed in blocks.
    committed: AddressToNonce,
    /// Provisionally incremented nonces during block creation.
    staged: AddressToNonce,
}

impl MempoolState {
    fn new(committed_nonce_retention_block_count: usize) -> Self {
        MempoolState {
            commit_history: CommitHistory::new(committed_nonce_retention_block_count),
            committed: HashMap::new(),
            staged: HashMap::new(),
        }
    }

    fn resolve_nonce(&self, address: ContractAddress, incoming_account_nonce: Nonce) -> Nonce {
        self.staged
            .get(&address)
            .or_else(|| self.committed.get(&address))
            .copied()
            .unwrap_or(incoming_account_nonce)
    }

    fn contains_account(&self, address: ContractAddress) -> bool {
        self.staged.contains_key(&address) || self.committed.contains_key(&address)
    }

    fn stage(&mut self, tx_reference: &TransactionReference) -> MempoolResult<()> {
        let next_nonce = try_increment_nonce(tx_reference.nonce)?;
        if let Some(existing_nonce) = self.staged.insert(tx_reference.address, next_nonce) {
            assert_eq!(
                try_increment_nonce(existing_nonce)?,
                next_nonce,
                "Staged nonce should be an increment of an existing nonce."
            );
        }

        Ok(())
    }

    fn commit(&mut self, address_to_nonce: AddressToNonce) -> Vec<ContractAddress> {
        let addresses_to_rewind: Vec<_> = self
            .staged
            .keys()
            .filter(|&key| !address_to_nonce.contains_key(key))
            .copied()
            .collect();

        self.committed.extend(address_to_nonce.clone());
        self.staged.clear();

        // Add the commit event to the history.
        // If an old event has been removed (due to history size limit), delete the associated
        // committed nonces.
        let removed_commit = self.commit_history.push(address_to_nonce);
        for (address, removed_nonce) in removed_commit {
            let last_committed_nonce = *self
                .committed
                .get(&address)
                .expect("Account in commit history must appear in the committed nonces.");
            if last_committed_nonce == removed_nonce {
                self.committed.remove(&address);
            }
        }

        addresses_to_rewind
    }

    fn validate_incoming_tx(
        &self,
        tx_reference: TransactionReference,
        incoming_account_nonce: Nonce,
    ) -> MempoolResult<()> {
        let TransactionReference { address, nonce: tx_nonce, .. } = tx_reference;
        let account_nonce = self.resolve_nonce(address, incoming_account_nonce);
        if tx_nonce < account_nonce {
            return Err(MempoolError::NonceTooOld { address, nonce: tx_nonce });
        }

        Ok(())
    }

    fn validate_commitment(&self, address: ContractAddress, next_nonce: Nonce) {
        // FIXME: Remove after first POC.
        // If commit_block wants to decrease the stored account nonce this can mean one of two
        // things:
        // 1. this is a reorg, which should be handled by a dedicated TBD mechanism and not inside
        //    commit_block
        // 2. the stored nonce originated from add_tx, so should be treated as tentative due to
        //    possible races with the gateway; these types of nonces should be tagged somehow so
        //    that commit_block can override them. Regardless, in the first POC this cannot happen
        //    because the GW nonces are always 1.
        if let Some(&committed_nonce) = self.committed.get(&address) {
            assert!(committed_nonce <= next_nonce, "NOT SUPPORTED YET {address:?} {next_nonce:?}.")
        }
    }
}

pub struct Mempool {
    config: MempoolConfig,
    // TODO(AlonH): add docstring explaining visibility and coupling of the fields.
    // Declare transactions that are waiting to be added to the tx pool after a delay.
    delayed_declares: VecDeque<(Instant, AddTransactionArgs)>,
    // All transactions currently held in the mempool (excluding the delayed declares).
    tx_pool: TransactionPool,
    // Transactions eligible for sequencing.
    tx_queue: TransactionQueue,
    state: MempoolState,
    clock: Arc<dyn Clock>,
}

impl Mempool {
    pub fn new(config: MempoolConfig, clock: Arc<dyn Clock>) -> Self {
        Mempool {
            config: config.clone(),
            delayed_declares: VecDeque::new(),
            tx_pool: TransactionPool::new(clock.clone()),
            tx_queue: TransactionQueue::default(),
            state: MempoolState::new(config.committed_nonce_retention_block_count),
            clock,
        }
    }

    pub fn priority_queue_len(&self) -> usize {
        self.tx_queue.priority_queue_len()
    }

    pub fn pending_queue_len(&self) -> usize {
        self.tx_queue.pending_queue_len()
    }

    pub fn tx_pool_len(&self) -> usize {
        self.tx_pool.capacity()
    }

    pub fn delayed_declares_len(&self) -> usize {
        self.delayed_declares.len()
    }

    /// Returns an iterator of the current eligible transactions for sequencing, ordered by their
    /// priority.
    pub fn iter(&self) -> impl Iterator<Item = &TransactionReference> {
        self.tx_queue.iter_over_ready_txs()
    }

    /// Retrieves up to `n_txs` transactions with the highest priority from the mempool.
    /// Transactions are guaranteed to be unique across calls until the block in-progress is
    /// created.
    // TODO(AlonH): Consider renaming to `pop_txs` to be more consistent with the standard library.
    #[instrument(skip(self), err)]
    pub fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalRpcTransaction>> {
        self.add_ready_declares();
        let mut eligible_tx_references: Vec<TransactionReference> = Vec::with_capacity(n_txs);
        let mut n_remaining_txs = n_txs;

        while n_remaining_txs > 0 && self.tx_queue.has_ready_txs() {
            let chunk = self.tx_queue.pop_ready_chunk(n_remaining_txs);
            let valid_txs = self.prune_expired_nonqueued_txs(chunk);

            self.enqueue_next_eligible_txs(&valid_txs)?;
            n_remaining_txs -= valid_txs.len();
            eligible_tx_references.extend(valid_txs);
        }

        // Update the mempool state with the given transactions' nonces.
        for tx_reference in &eligible_tx_references {
            self.state.stage(tx_reference)?;
        }

        info!(
            "Returned {} out of {n_txs} transactions, ready for sequencing.",
            eligible_tx_references.len()
        );

        metric_set_get_txs_size(eligible_tx_references.len());
        self.update_state_metrics();

        Ok(eligible_tx_references
            .iter()
            .map(|tx_reference| {
                self.tx_pool
                    .get_by_tx_hash(tx_reference.tx_hash)
                    .expect("Transaction hash from queue must appear in pool.")
            })
            .cloned() // Soft-delete: return without deleting from mempool.
            .collect())
    }

    /// Adds a new transaction to the mempool.
    #[instrument(
        skip(self, args),
        fields( // Log subset of (informative) fields.
            tx_nonce = %args.tx.nonce(),
            tx_hash = %args.tx.tx_hash,
            tx_tip = %args.tx.tip(),
            tx_max_l2_gas_price = %args.tx.resource_bounds().l2_gas.max_price_per_unit,
            account_state = %args.account_state
        ),
        err
    )]
    pub fn add_tx(&mut self, args: AddTransactionArgs) -> MempoolResult<()> {
        let mut metric_handle = MempoolMetricHandle::new(&args.tx.tx);
        metric_handle.count_transaction_received();

        // First remove old transactions from the pool.
        self.remove_expired_txs();
        self.add_ready_declares();

        let tx_reference = TransactionReference::new(&args.tx);
        self.validate_incoming_tx(tx_reference, args.account_state.nonce)?;
        self.handle_fee_escalation(&args.tx)?;

        metric_handle.transaction_inserted();

        if let InternalRpcTransactionWithoutTxHash::Declare(_) = &args.tx.tx {
            self.delayed_declares.push_back((self.clock.now(), args));
        } else {
            self.add_tx_inner(args);
        }

        self.update_state_metrics();
        Ok(())
    }

    fn add_tx_inner(&mut self, args: AddTransactionArgs) {
        let AddTransactionArgs { tx, account_state } = args;
        info!("Adding transaction to mempool.");
        trace!("{tx:#?}");

        let tx_reference = TransactionReference::new(&tx);

        self.tx_pool.insert(tx).expect("Duplicate transactions should error in validation stage.");

        let AccountState { address, nonce: incoming_account_nonce } = account_state;
        let account_nonce = self.state.resolve_nonce(address, incoming_account_nonce);
        if tx_reference.nonce == account_nonce {
            self.tx_queue.remove(address);
            self.tx_queue.insert(tx_reference);
        }
    }

    fn add_ready_declares(&mut self) {
        let now = self.clock.now();
        while let Some((submission_time, _args)) = self.delayed_declares.front() {
            if now - *submission_time < self.config.declare_delay {
                break;
            }
            let (_submission_time, args) =
                self.delayed_declares.pop_front().expect("Delay declare should exist.");
            self.add_tx_inner(args);
        }
        self.update_state_metrics();
    }

    /// Update the mempool's internal state according to the committed block (resolves nonce gaps,
    /// updates account balances).
    #[instrument(skip(self, args))]
    pub fn commit_block(&mut self, args: CommitBlockArgs) {
        let CommitBlockArgs { address_to_nonce, rejected_tx_hashes } = args;
        debug!(
            "Committing block with {} addresses and {} rejected tx to the mempool.",
            address_to_nonce.len(),
            rejected_tx_hashes.len()
        );

        // Align mempool data to committed nonces.
        for (&address, &next_nonce) in &address_to_nonce {
            self.validate_commitment(address, next_nonce);

            // Maybe remove out-of-date transactions.
            if self
                .tx_queue
                .get_nonce(address)
                .is_some_and(|queued_nonce| queued_nonce != next_nonce)
            {
                assert!(self.tx_queue.remove(address), "Expected to remove address from queue.");
            }

            // Remove from pool.
            let n_removed_txs = self.tx_pool.remove_up_to_nonce(address, next_nonce);
            metric_count_committed_txs(n_removed_txs);

            // Maybe close nonce gap.
            if self.tx_queue.get_nonce(address).is_none() {
                if let Some(tx_reference) =
                    self.tx_pool.get_by_address_and_nonce(address, next_nonce)
                {
                    self.tx_queue.insert(tx_reference);
                }
            }
        }

        // Commit block and rewind nonces of addresses that were not included in block.
        let addresses_to_rewind = self.state.commit(address_to_nonce);
        for address in addresses_to_rewind {
            // Account nonce is the minimal nonce of this address: it was proposed but not included.
            let tx_reference =
                self.tx_pool.account_txs_sorted_by_nonce(address).next().unwrap_or_else(|| {
                    panic!("Address {address} should appear in transaction pool.")
                });
            self.tx_queue.remove(address);
            self.tx_queue.insert(*tx_reference);
        }

        debug!("Aligned mempool to committed nonces.");

        // Remove rejected transactions from the mempool.
        metric_count_rejected_txs(rejected_tx_hashes.len());
        for tx_hash in rejected_tx_hashes {
            if let Ok(tx) = self.tx_pool.remove(tx_hash) {
                self.tx_queue.remove(tx.contract_address());
            } else {
                continue; // Transaction hash unknown to mempool, from a different node.
            };

            // TODO(clean_accounts): remove address with no transactions left after a block cycle /
            // TTL.
        }
        debug!("Removed rejected transactions known to mempool.");

        self.update_state_metrics();
    }

    pub fn account_tx_in_pool_or_recent_block(&self, account_address: ContractAddress) -> bool {
        self.state.contains_account(account_address)
            || self.tx_pool.contains_account(account_address)
    }

    fn validate_incoming_tx(
        &self,
        tx_reference: TransactionReference,
        incoming_account_nonce: Nonce,
    ) -> MempoolResult<()> {
        if self.tx_pool.get_by_tx_hash(tx_reference.tx_hash).is_ok() {
            return Err(MempoolError::DuplicateTransaction { tx_hash: tx_reference.tx_hash });
        }
        self.state.validate_incoming_tx(tx_reference, incoming_account_nonce)
    }

    /// Validates that the given transaction does not front run a delayed declare. This means in
    /// particular that no fee escalation can occur to a declare that is being delayed.
    fn validate_no_delayed_declare_front_run(
        &self,
        tx_reference: TransactionReference,
    ) -> MempoolResult<()> {
        if self.delayed_declares.iter().any(|(_, tx_args)| {
            let tx = &tx_args.tx;
            tx.contract_address() == tx_reference.address && tx.nonce() == tx_reference.nonce
        }) {
            return Err(MempoolError::DuplicateNonce {
                address: tx_reference.address,
                nonce: tx_reference.nonce,
            });
        }
        Ok(())
    }

    fn validate_commitment(&self, address: ContractAddress, next_nonce: Nonce) {
        self.state.validate_commitment(address, next_nonce);
    }

    /// Updates the gas price threshold for transactions that are eligible for sequencing.
    pub fn update_gas_price(&mut self, threshold: NonzeroGasPrice) {
        self.tx_queue.update_gas_price_threshold(threshold);
        self.update_state_metrics();
    }

    fn enqueue_next_eligible_txs(&mut self, txs: &[TransactionReference]) -> MempoolResult<()> {
        for tx in txs {
            let current_account_state = AccountState { address: tx.address, nonce: tx.nonce };

            if let Some(next_tx_reference) =
                self.tx_pool.get_next_eligible_tx(current_account_state)?
            {
                self.tx_queue.insert(next_tx_reference);
            }
        }

        Ok(())
    }

    #[instrument(level = "debug", skip(self, incoming_tx), err)]
    fn handle_fee_escalation(&mut self, incoming_tx: &InternalRpcTransaction) -> MempoolResult<()> {
        let incoming_tx_reference = TransactionReference::new(incoming_tx);
        let TransactionReference { address, nonce, .. } = incoming_tx_reference;

        self.validate_no_delayed_declare_front_run(incoming_tx_reference)?;

        if !self.config.enable_fee_escalation {
            if self.tx_pool.get_by_address_and_nonce(address, nonce).is_some() {
                return Err(MempoolError::DuplicateNonce { address, nonce });
            };

            return Ok(());
        }

        let Some(existing_tx_reference) = self.tx_pool.get_by_address_and_nonce(address, nonce)
        else {
            // Replacement irrelevant: no existing transaction with the same nonce for address.
            return Ok(());
        };

        if !self.should_replace_tx(&existing_tx_reference, &incoming_tx_reference) {
            debug!(
                "{existing_tx_reference} was not replaced by {incoming_tx_reference} due to
                insufficient fee escalation."
            );
            // TODO(Elin): consider adding a more specific error type / message.
            return Err(MempoolError::DuplicateNonce { address, nonce });
        }

        debug!("{existing_tx_reference} will be replaced by {incoming_tx_reference}.");

        self.tx_queue.remove_txs(&[existing_tx_reference]);
        self.tx_pool
            .remove(existing_tx_reference.tx_hash)
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
            [existing_tx, incoming_tx].map(|tx| tx.max_l2_gas_price.get().0);

        self.increased_enough(existing_tip, incoming_tip)
            && self.increased_enough(existing_max_l2_gas_price, incoming_max_l2_gas_price)
    }

    fn increased_enough(&self, existing_value: u128, incoming_value: u128) -> bool {
        let percentage = u128::from(self.config.fee_escalation_percentage);

        // Note: To reduce precision loss, we first multiply by the percentage and then divide by
        // 100. This could cause an overflow and an automatic rejection of the transaction, but the
        // values aren't expected to be large enough for this to be an issue.
        let Some(escalation_qualified_value) = existing_value
            .checked_mul(percentage)
            .map(|v| v / 100)
            .and_then(|increase| existing_value.checked_add(increase))
        else {
            // Overflow occurred during calculation; reject the transaction.
            return false;
        };

        incoming_value >= escalation_qualified_value
    }

    fn remove_expired_txs(&mut self) {
        let removed_txs =
            self.tx_pool.remove_txs_older_than(self.config.transaction_ttl, &self.state.staged);
        self.tx_queue.remove_txs(&removed_txs);

        metric_count_expired_txs(removed_txs.len());
        self.update_state_metrics();
    }

    /// Given a chunk of transactions, removes from the pool those that are old, and returns the
    /// remaining valid ones.
    /// Note: This function assumes that the given transactions were already removed from the queue.
    fn prune_expired_nonqueued_txs(
        &mut self,
        txs: Vec<TransactionReference>,
    ) -> Vec<TransactionReference> {
        // Divide the chunk into transactions that are old and no longer valid and those that
        // remain valid.
        let submission_cutoff_time = self.clock.now() - self.config.transaction_ttl;
        let (old_txs, valid_txs): (Vec<_>, Vec<_>) = txs.into_iter().partition(|tx| {
            let tx_submission_time = self
                .tx_pool
                .get_submission_time(tx.tx_hash)
                .expect("Transaction hash from queue must appear in pool.");
            tx_submission_time < submission_cutoff_time
        });

        // Remove old transactions from the pool.
        metric_count_expired_txs(old_txs.len());
        for tx in old_txs {
            self.tx_pool
                .remove(tx.tx_hash)
                .expect("Transaction hash from queue must appear in pool.");
        }

        valid_txs
    }

    pub fn get_mempool_snapshot(&self) -> MempoolResult<MempoolSnapshot> {
        Ok(MempoolSnapshot {
            transactions: self.tx_pool.get_chronological_txs_hashes(),
            transaction_queue: self.tx_queue.get_queue_snapshot(),
        })
    }

    #[cfg(test)]
    fn content(&self) -> MempoolContent {
        MempoolContent {
            tx_pool: self.tx_pool.tx_pool(),
            priority_txs: self.tx_queue.iter_over_ready_txs().cloned().collect(),
            pending_txs: self.tx_queue.pending_txs(),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
struct MempoolContent {
    tx_pool: HashMap<TransactionHash, InternalRpcTransaction>,
    priority_txs: Vec<TransactionReference>,
    pending_txs: Vec<TransactionReference>,
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
    pub max_l2_gas_price: NonzeroGasPrice,
}

impl TransactionReference {
    pub fn new(tx: &InternalRpcTransaction) -> Self {
        TransactionReference {
            address: tx.contract_address(),
            nonce: tx.nonce(),
            tx_hash: tx.tx_hash(),
            tip: tx.tip(),
            max_l2_gas_price: NonzeroGasPrice::new(tx.resource_bounds().l2_gas.max_price_per_unit)
                .expect("Max L2 gas price must be non-zero."),
        }
    }
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
