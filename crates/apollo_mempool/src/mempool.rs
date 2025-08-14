use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{
    AccountState,
    AddTransactionArgs,
    CommitBlockArgs,
    MempoolResult,
    MempoolSnapshot,
    MempoolStateSnapshot,
};
use apollo_time::time::{Clock, DateTime};
use indexmap::IndexSet;
use rand::{thread_rng, Rng};
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::{InternalRpcTransaction, InternalRpcTransactionWithoutTxHash};
use starknet_api::transaction::fields::Tip;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, info, instrument, trace};

use crate::config::MempoolConfig;
use crate::metrics::{
    metric_count_committed_txs,
    metric_count_expired_txs,
    metric_count_rejected_txs,
    metric_set_get_txs_size,
    MempoolMetricHandle,
    MEMPOOL_DELAYED_DECLARES_SIZE,
    MEMPOOL_EVICTIONS_COUNT,
    MEMPOOL_PENDING_QUEUE_SIZE,
    MEMPOOL_POOL_SIZE,
    MEMPOOL_PRIORITY_QUEUE_SIZE,
    MEMPOOL_TOTAL_SIZE_BYTES,
};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;
use crate::utils::try_increment_nonce;

#[cfg(test)]
#[path = "mempool_test.rs"]
pub mod mempool_test;

#[cfg(test)]
#[path = "mempool_flow_tests.rs"]
pub mod mempool_flow_tests;

type AddressToNonce = HashMap<ContractAddress, Nonce>;
type AccountsWithGap = IndexSet<ContractAddress>;

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
struct CommitHistory {
    commits: VecDeque<AddressToNonce>,
}

impl CommitHistory {
    fn new(capacity: usize) -> Self {
        CommitHistory { commits: std::iter::repeat_n(AddressToNonce::new(), capacity).collect() }
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

    /// Returns the most updated Nonce (including staged) for the address. If no value is found for
    /// address, incoming_account_nonce is returned.
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

    /// Updates the committed nonces, and returns the addresses which need to be rewinded (i.e.
    /// addressed which were staged but did not make to the commit).
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
            return Err(MempoolError::NonceTooOld { address, tx_nonce, account_nonce });
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

    pub fn state_snapshot(&self) -> MempoolStateSnapshot {
        MempoolStateSnapshot { committed: self.committed.clone(), staged: self.staged.clone() }
    }
}

// A queue to hold transactions that are waiting to be added to the tx pool.
struct AddTransactionQueue {
    elements: VecDeque<(DateTime, AddTransactionArgs)>,
    // Keeps track of the total size of the transactions in this queue.
    size_in_bytes: u64,
}

impl AddTransactionQueue {
    fn new() -> Self {
        AddTransactionQueue { elements: VecDeque::new(), size_in_bytes: 0 }
    }

    fn push_back(&mut self, submission_time: DateTime, args: AddTransactionArgs) {
        self.size_in_bytes = self
            .size_in_bytes
            .checked_add(args.tx.total_bytes())
            .expect("Overflow when adding a transaction to AddTransactionQueue.");
        self.elements.push_back((submission_time, args));
    }

    fn pop_front(&mut self) -> Option<(DateTime, AddTransactionArgs)> {
        let removed_element = self.elements.pop_front();
        if let Some((_, args)) = &removed_element {
            self.size_in_bytes = self
                .size_in_bytes
                .checked_sub(args.tx.total_bytes())
                .expect("Underflow when removing a transaction from AddTransactionQueue.");
        }
        removed_element
    }

    fn front(&self) -> Option<&(DateTime, AddTransactionArgs)> {
        self.elements.front()
    }

    fn contains(&self, contract_address: ContractAddress, nonce: Nonce) -> bool {
        self.elements.iter().any(|(_, tx_args)| {
            let tx = &tx_args.tx;
            tx.contract_address() == contract_address && tx.nonce() == nonce
        })
    }

    fn len(&self) -> usize {
        self.elements.len()
    }

    fn size_in_bytes(&self) -> u64 {
        self.size_in_bytes
    }
}

pub struct Mempool {
    config: MempoolConfig,
    // TODO(AlonH): add docstring explaining visibility and coupling of the fields.
    // Declare transactions that are waiting to be added to the tx pool after a delay.
    delayed_declares: AddTransactionQueue,
    // All transactions currently held in the mempool (excluding the delayed declares).
    tx_pool: TransactionPool,
    // Transactions eligible for sequencing.
    tx_queue: TransactionQueue,
    // Accounts whose lowest transaction nonce is greater than the account nonce, which are
    // therefore candidates for eviction.
    accounts_with_gap: AccountsWithGap,
    state: MempoolState,
    clock: Arc<dyn Clock>,
}

impl Mempool {
    pub fn new(config: MempoolConfig, clock: Arc<dyn Clock>) -> Self {
        Mempool {
            config: config.clone(),
            delayed_declares: AddTransactionQueue::new(),
            tx_pool: TransactionPool::new(clock.clone()),
            tx_queue: TransactionQueue::default(),
            accounts_with_gap: AccountsWithGap::new(),
            state: MempoolState::new(config.committed_nonce_retention_block_count),
            clock,
        }
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

        let mut account_nonce_updates = AddressToNonce::new();
        while n_remaining_txs > 0 && self.tx_queue.has_ready_txs() {
            let chunk = self.tx_queue.pop_ready_chunk(n_remaining_txs);
            let (valid_txs, expired_txs_updates) = self.prune_expired_nonqueued_txs(chunk);
            account_nonce_updates.extend(expired_txs_updates);

            self.enqueue_next_eligible_txs(&valid_txs)?;
            n_remaining_txs -= valid_txs.len();
            eligible_tx_references.extend(valid_txs);
        }

        // Update the mempool state with the given transactions' nonces.
        for tx_reference in &eligible_tx_references {
            self.state.stage(tx_reference)?;
        }

        let n_returned_txs = eligible_tx_references.len();
        if n_returned_txs != 0 {
            info!("Returned {n_returned_txs} out of {n_txs} transactions, ready for sequencing.");
            debug!(
                "Returned mempool txs: {:?}",
                eligible_tx_references.iter().map(|tx| tx.tx_hash).collect::<Vec<_>>()
            );
        }

        metric_set_get_txs_size(n_returned_txs);
        self.update_state_metrics();
        self.update_accounts_with_gap(account_nonce_updates);

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
        let mut account_nonce_updates = self.remove_expired_txs();
        self.add_ready_declares();

        let tx_reference = TransactionReference::new(&args.tx);
        self.validate_incoming_tx(tx_reference, args.account_state.nonce)?;
        self.handle_fee_escalation(&args.tx)?;

        if self.exceeds_capacity(&args.tx) {
            self.handle_capacity_overflow(&args.tx, args.account_state.nonce)?;
        }

        metric_handle.transaction_inserted();

        // May override a removed queued nonce with the received account nonce or the account's
        // state nonce.
        account_nonce_updates.insert(
            args.account_state.address,
            self.state.resolve_nonce(args.account_state.address, args.account_state.nonce),
        );

        if let InternalRpcTransactionWithoutTxHash::Declare(_) = &args.tx.tx {
            self.delayed_declares.push_back(self.clock.now(), args);
        } else {
            self.add_tx_inner(args);
        }

        self.update_state_metrics();
        self.update_accounts_with_gap(account_nonce_updates);
        Ok(())
    }

    fn insert_to_tx_queue(&mut self, tx_reference: TransactionReference) {
        self.tx_queue.insert(tx_reference, self.config.validate_resource_bounds);
    }

    fn add_tx_inner(&mut self, args: AddTransactionArgs) {
        let AddTransactionArgs { tx, account_state } = args;
        info!("Adding transaction to mempool.");
        trace!("{tx:#?}");

        let tx_reference = TransactionReference::new(&tx);

        self.tx_pool
            .insert(tx)
            .expect("Duplicate transactions should cause an error during the validation stage.");

        let AccountState { address, nonce: incoming_account_nonce } = account_state;
        let account_nonce = self.state.resolve_nonce(address, incoming_account_nonce);
        if tx_reference.nonce == account_nonce {
            // Remove queued transactions the account might have. This includes old nonce
            // transactions that have become obsolete; those with an equal nonce should
            // already have been removed in `handle_fee_escalation`.
            self.tx_queue.remove(address);
            self.insert_to_tx_queue(tx_reference);
        }
    }

    fn add_ready_declares(&mut self) {
        let now = self.clock.now();
        while let Some((submission_time, _args)) = self.delayed_declares.front() {
            if now - self.config.declare_delay < *submission_time {
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

        let mut committed_nonce_updates = AddressToNonce::new();
        // Align mempool data to committed nonces.
        for (&address, &next_nonce) in &address_to_nonce {
            self.validate_commitment(address, next_nonce);
            committed_nonce_updates.insert(address, next_nonce);

            // Maybe remove out-of-date transactions.
            if self
                .tx_queue
                .get_nonce(address)
                .is_some_and(|queued_nonce| queued_nonce != next_nonce)
            {
                assert!(self.tx_queue.remove(address), "Expected to remove address from queue.");
            }

            // Remove from pool.
            let n_removed_txs = self.tx_pool.remove_up_to_nonce_when_committed(address, next_nonce);
            metric_count_committed_txs(n_removed_txs);

            // Maybe close nonce gap.
            if self.tx_queue.get_nonce(address).is_none() {
                if let Some(tx_reference) =
                    self.tx_pool.get_by_address_and_nonce(address, next_nonce)
                {
                    self.insert_to_tx_queue(tx_reference);
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
            self.insert_to_tx_queue(*tx_reference);
        }

        debug!("Aligned mempool to committed nonces.");

        // Remove rejected transactions from the mempool.
        if !rejected_tx_hashes.is_empty() {
            debug!("Removed rejected transactions from mempool: {:?}", rejected_tx_hashes);
        }
        metric_count_rejected_txs(rejected_tx_hashes.len());
        let mut account_nonce_updates = AddressToNonce::new();
        for tx_hash in rejected_tx_hashes {
            if let Ok(tx) = self.tx_pool.remove(tx_hash) {
                self.tx_queue.remove(tx.contract_address());
                account_nonce_updates
                    .entry(tx.contract_address())
                    .and_modify(|nonce| *nonce = (*nonce).min(tx.nonce()))
                    .or_insert(tx.nonce());
            } else {
                continue; // Transaction hash unknown to mempool, from a different node.
            };

            // TODO(clean_accounts): remove address with no transactions left after a block cycle /
            // TTL.
        }

        // Committed nonces should overwrite rejected transactions.
        account_nonce_updates.extend(committed_nonce_updates);

        self.update_state_metrics();
        self.update_accounts_with_gap(account_nonce_updates);
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
        if self.delayed_declares.contains(tx_reference.address, tx_reference.nonce) {
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
    pub fn update_gas_price(&mut self, threshold: GasPrice) {
        self.tx_queue.update_gas_price_threshold(threshold);
        self.update_state_metrics();
    }

    fn enqueue_next_eligible_txs(&mut self, txs: &[TransactionReference]) -> MempoolResult<()> {
        for tx in txs {
            let current_account_state = AccountState { address: tx.address, nonce: tx.nonce };

            if let Some(next_tx_reference) =
                self.tx_pool.get_next_eligible_tx(current_account_state)?
            {
                self.insert_to_tx_queue(next_tx_reference);
            }
        }

        Ok(())
    }

    /// If this transaction is already in the pool but the fees have increased beyond the thereshold
    /// in the config, remove the existing transaction from the queue and the pool.
    /// Note: This method will **not** add the new incoming transaction.
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
            [existing_tx, incoming_tx].map(|tx| tx.max_l2_gas_price.0);

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

    fn remove_expired_txs(&mut self) -> AddressToNonce {
        let removed_txs =
            self.tx_pool.remove_txs_older_than(self.config.transaction_ttl, &self.state.staged);
        let queued_txs = self.tx_queue.remove_txs(&removed_txs);

        metric_count_expired_txs(removed_txs.len());
        self.update_state_metrics();
        queued_txs
            .into_iter()
            .map(|tx| (tx.address, self.state.resolve_nonce(tx.address, tx.nonce)))
            .collect::<AddressToNonce>()
    }

    /// Given a chunk of transactions, removes from the pool those that are old, and returns the
    /// remaining valid ones.
    /// Note: This function assumes that the given transactions were already removed from the queue.
    fn prune_expired_nonqueued_txs(
        &mut self,
        txs: Vec<TransactionReference>,
    ) -> (Vec<TransactionReference>, AddressToNonce) {
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
        let account_nonce_updates: AddressToNonce = old_txs
            .into_iter()
            .map(|tx| {
                self.tx_pool
                    .remove(tx.tx_hash)
                    .expect("Transaction hash from queue must appear in pool.");
                (tx.address, self.state.resolve_nonce(tx.address, tx.nonce))
            })
            .collect();

        (valid_txs, account_nonce_updates)
    }

    pub fn mempool_snapshot(&self) -> MempoolResult<MempoolSnapshot> {
        Ok(MempoolSnapshot {
            transactions: self.tx_pool.chronological_txs_hashes(),
            delayed_declares: self
                .delayed_declares
                .elements
                .iter()
                .map(|(_, args)| args.tx.tx_hash)
                .collect(),
            transaction_queue: self.tx_queue.queue_snapshot(),
            mempool_state: self.state.state_snapshot(),
        })
    }

    fn size_in_bytes(&self) -> u64 {
        self.tx_pool.size_in_bytes() + self.delayed_declares.size_in_bytes()
    }

    // Returns true if the mempool will exceeds its capacity by adding the given transaction.
    fn exceeds_capacity(&self, tx: &InternalRpcTransaction) -> bool {
        self.size_in_bytes() + tx.total_bytes() > self.config.capacity_in_bytes
    }

    fn update_accounts_with_gap(&mut self, address_to_nonce: AddressToNonce) {
        for (address, account_nonce) in address_to_nonce {
            // Assumption: Future declares are not allowed — their nonce must match the account
            // nonce, so they fill a gap if one exists.
            if self.delayed_declares.contains(address, account_nonce) {
                self.accounts_with_gap.swap_remove(&address);
                continue;
            }

            // Gap exists when lowest transaction nonce is higher than account nonce.
            let gap_exists = match self.tx_pool.get_lowest_nonce(address) {
                Some(lowest_nonce) => account_nonce < lowest_nonce,
                None => false, // No transactions for the account, so no gap.
            };

            // Update the eviction tracking set accordingly.
            if gap_exists {
                self.accounts_with_gap.insert(address);
            } else {
                self.accounts_with_gap.swap_remove(&address);
            }
        }
    }

    pub fn get_evictable_account(&self) -> Option<ContractAddress> {
        let len = self.accounts_with_gap.len();
        if len == 0 {
            return None;
        }
        let random_index = thread_rng().gen_range(0..len);
        self.accounts_with_gap.get_index(random_index).copied()
    }

    // Attempts to make space for a new transaction by evicting existing transactions.
    // Returns true if enough space was freed, false otherwise.
    pub fn try_make_space(&mut self, required_space: u64) -> bool {
        let mut total_space_freed = 0;

        while total_space_freed < required_space {
            let Some(address) = self.get_evictable_account() else {
                return false;
            };

            let txs: Vec<_> = self.tx_pool.account_txs_sorted_by_nonce(address).copied().collect();
            for tx_ref in txs.iter().rev() {
                let tx = self
                    .tx_pool
                    .remove(tx_ref.tx_hash)
                    .expect("Transaction must exist in the pool.");
                total_space_freed += tx.total_bytes();
                MEMPOOL_EVICTIONS_COUNT.increment(1);
                if total_space_freed >= required_space {
                    break;
                }
            }

            // Clean up if account is now empty.
            if !self.tx_pool.contains_account(address) {
                self.accounts_with_gap.swap_remove(&address);
            }
        }

        true
    }

    fn handle_capacity_overflow(
        &mut self,
        tx: &InternalRpcTransaction,
        account_nonce: Nonce,
    ) -> Result<(), MempoolError> {
        let address = tx.contract_address();

        let account_has_gap = self.accounts_with_gap.contains(&address);
        let account_has_txs = self.tx_pool.contains_account(address);
        let closing_gap = tx.nonce() == account_nonce;
        let creating_gap = (account_has_gap || !account_has_txs) && !closing_gap;

        if !creating_gap && self.try_make_space(tx.total_bytes()) {
            return Ok(());
        }

        Err(MempoolError::MempoolFull)
    }

    #[cfg(test)]
    fn content(&self) -> MempoolContent {
        MempoolContent {
            tx_pool: self.tx_pool.tx_pool(),
            priority_txs: self.tx_queue.iter_over_ready_txs().cloned().collect(),
            pending_txs: self.tx_queue.pending_txs(),
        }
    }

    #[cfg(test)]
    fn accounts_with_gap(&self) -> &AccountsWithGap {
        &self.accounts_with_gap
    }

    fn update_state_metrics(&self) {
        MEMPOOL_POOL_SIZE.set_lossy(self.tx_pool.len());
        MEMPOOL_PRIORITY_QUEUE_SIZE.set_lossy(self.tx_queue.priority_queue_len());
        MEMPOOL_PENDING_QUEUE_SIZE.set_lossy(self.tx_queue.pending_queue_len());
        MEMPOOL_DELAYED_DECLARES_SIZE.set_lossy(self.delayed_declares.len());
        MEMPOOL_TOTAL_SIZE_BYTES.set_lossy(self.size_in_bytes());
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
    pub max_l2_gas_price: GasPrice,
}

impl TransactionReference {
    pub fn new(tx: &InternalRpcTransaction) -> Self {
        TransactionReference {
            address: tx.contract_address(),
            nonce: tx.nonce(),
            tx_hash: tx.tx_hash(),
            tip: tx.tip(),
            max_l2_gas_price: tx.resource_bounds().l2_gas.max_price_per_unit,
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
