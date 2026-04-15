use std::collections::{HashMap, HashSet, VecDeque};

use apollo_mempool_types::mempool_types::{TransactionQueueSnapshot, TxBlockMetadata};
use indexmap::IndexSet;
use starknet_api::block::{BlockNumber, GasPrice, UnixTimestamp};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;
use tracing::debug;

use crate::mempool::TransactionReference;
use crate::transaction_queue_trait::{BlockMetadata, RewindData, TransactionQueueTrait};

/// A FIFO (First-In-First-Out) transaction queue that orders transactions by arrival time.
/// Used in Echonet mode to preserve the original transaction order from the source chain.
#[derive(Clone, Copy, Debug)]
struct FifoTransaction {
    tx_reference: TransactionReference,
    timestamp: UnixTimestamp,
    block_number: BlockNumber,
}

#[derive(Clone, Copy, Debug)]
struct CurrentProposalState {
    timestamp: UnixTimestamp,
    expected_block_number: BlockNumber,
    emit_empty_block: bool,
}

impl CurrentProposalState {
    fn matches_tx(&self, tx: &FifoTransaction) -> bool {
        !self.emit_empty_block
            && tx.timestamp == self.timestamp
            && tx.block_number == self.expected_block_number
    }

    fn advance_block_if_drained(&mut self, remaining_queue: &VecDeque<FifoTransaction>) {
        let has_more_txs_in_current_block = remaining_queue
            .front()
            .is_some_and(|head| head.block_number == self.expected_block_number);

        if has_more_txs_in_current_block {
            return;
        }

        self.expected_block_number = self
            .expected_block_number
            .next()
            .expect("Block number overflow while advancing expected block after pop.");
        self.emit_empty_block = false;
    }
}

enum InsertionSide {
    Front,
    Back,
}

impl InsertionSide {
    fn push(self, queue: &mut VecDeque<FifoTransaction>, tx: FifoTransaction) {
        match self {
            InsertionSide::Front => queue.push_front(tx),
            InsertionSide::Back => queue.push_back(tx),
        }
    }
}

#[derive(Debug)]
pub struct FifoTransactionQueue {
    // Queue of transactions ordered by arrival time (FIFO).
    queue: VecDeque<FifoTransaction>,
    // Transactions that were returned by get_txs and may need rewind during commit.
    staged_txs: Vec<FifoTransaction>,
    // Temporary map from transaction hash to metadata before the transaction is inserted to
    // queue.
    pending_metadata: HashMap<TransactionHash, TxBlockMetadata>,
    // Proposal state set by resolve_metadata(); gates which txs pop_ready_chunk() may return.
    current_proposal_state: Option<CurrentProposalState>,
}

impl FifoTransactionQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            staged_txs: Vec::new(),
            pending_metadata: HashMap::new(),
            current_proposal_state: None,
        }
    }

    fn group_staged_txs_by_address(
        &self,
        staged_txs: &[FifoTransaction],
    ) -> HashMap<ContractAddress, Vec<FifoTransaction>> {
        let mut grouped_by_address: HashMap<ContractAddress, Vec<FifoTransaction>> = HashMap::new();
        for &tx in staged_txs {
            grouped_by_address.entry(tx.tx_reference.address).or_default().push(tx);
        }
        grouped_by_address
    }

    fn collect_txs_to_rewind(
        &self,
        committed_nonces: &HashMap<ContractAddress, Nonce>,
        rejected_tx_hashes: &IndexSet<TransactionHash>,
    ) -> Vec<FifoTransaction> {
        // Step 1: group staged txs by address so rewind policy is evaluated per account.
        let staged_by_address = self.group_staged_txs_by_address(&self.staged_txs);
        // Step 2: decide which addresses should be rewound based on committed nonce + rejections.
        let addresses_to_rewind: HashSet<ContractAddress> = staged_by_address
            .iter()
            .filter(|(address, txs)| {
                if let Some(&nonce) = committed_nonces.get(address) {
                    // Address has committed txs in this block. if the next nonce is:
                    // - missing -> rewind this address
                    // - present + rejected -> do not rewind this address
                    // - present + not rejected -> rewind this address
                    txs.iter().find(|tx| tx.tx_reference.nonce == nonce).is_none_or(
                        |following_tx| {
                            !rejected_tx_hashes.contains(&following_tx.tx_reference.tx_hash)
                        },
                    )
                } else {
                    // Address has no committed txs in this block.
                    // Use first nonce to decide if the address should be rewound:
                    // - first nonce rejected -> do not rewind address
                    // - first nonce not rejected -> rewind address
                    let first_tx = txs
                        .iter()
                        .min_by_key(|tx| tx.tx_reference.nonce)
                        .expect("staged_by_address entry must have at least one transaction");
                    !rejected_tx_hashes.contains(&first_tx.tx_reference.tx_hash)
                }
            })
            .map(|(&address, _)| address)
            .collect();

        if addresses_to_rewind.is_empty() {
            return Vec::new();
        }

        // Step 3: staged txs to rewind: keep addresses marked for rewind, excluding txs already
        // committed in this block (nonce < committed nonce)
        self.staged_txs
            .iter()
            .filter(|tx| {
                let tx_ref = &tx.tx_reference;
                if !addresses_to_rewind.contains(&tx_ref.address) {
                    return false;
                }
                committed_nonces
                    .get(&tx_ref.address)
                    .is_none_or(|&committed_nonce| tx_ref.nonce >= committed_nonce)
            })
            .copied()
            .collect()
    }

    /// Syncs current_proposal_state from the head of the queue and returns
    /// the resulting (timestamp, block_number).
    fn sync_proposal_state_from_queue_front_tx(&mut self) -> BlockMetadata {
        let &front_tx = self
            .queue
            .front()
            .expect("FIFO sync_proposal_state_from_queue_front: queue must be non-empty");

        let Some(prev_state) = self.current_proposal_state else {
            let state = CurrentProposalState {
                timestamp: front_tx.timestamp,
                expected_block_number: front_tx.block_number,
                emit_empty_block: false,
            };
            self.current_proposal_state = Some(state);
            return BlockMetadata {
                timestamp: state.timestamp,
                block_number: Some(state.expected_block_number),
            };
        };

        let (expected_block_number, emit_empty_block) =
            if front_tx.block_number < prev_state.expected_block_number {
                // Rewind placed earlier-block txs at the head; realign to that block.
                (front_tx.block_number, false)
            } else if front_tx.block_number > prev_state.expected_block_number {
                // Head skips blocks, emit an empty block and advance by one.
                (
                    prev_state
                        .expected_block_number
                        .next()
                        .expect("Block number overflow while advancing expected block."),
                    true,
                )
            } else {
                // Head matches the expected block; process normally.
                (prev_state.expected_block_number, false)
            };

        let state = CurrentProposalState {
            timestamp: front_tx.timestamp,
            expected_block_number,
            emit_empty_block,
        };
        self.current_proposal_state = Some(state);
        BlockMetadata { timestamp: state.timestamp, block_number: Some(state.expected_block_number) }
    }
}

impl TransactionQueueTrait for FifoTransactionQueue {
    fn insert(&mut self, tx_reference: TransactionReference, _validate_resource_bounds: bool) {
        let metadata = self
            .pending_metadata
            .remove(&tx_reference.tx_hash)
            .expect("FIFO insert: transaction must have metadata set before insertion");
        // Add transaction to BACK of queue in FIFO order.
        let tx = FifoTransaction {
            tx_reference,
            timestamp: metadata.timestamp,
            block_number: metadata.block_number,
        };
        debug!(
            "FIFO insert: tx_hash={}, timestamp={}, block_number={}, queue_before={:?}",
            tx.tx_reference.tx_hash, tx.timestamp, tx.block_number, self.queue
        );
        InsertionSide::Back.push(&mut self.queue, tx);
    }

    fn pop_ready_chunk(&mut self, n_txs: usize) -> Vec<TransactionReference> {
        if self.queue.is_empty() {
            return Vec::new();
        }

        let Some(current_state) = self.current_proposal_state.as_mut() else {
            panic!(
                "FIFO pop_ready_chunk: missing proposal state; resolve_metadata must run before \
                 get_txs for this queue"
            );
        };

        let front_tx =
            self.queue.front().expect("FIFO pop_ready_chunk: queue non-empty after is_empty check");
        if !current_state.matches_tx(front_tx) {
            debug!(
                "FIFO pop_ready_chunk: empty chunk (head_block={:?}, expected_block={:?})",
                front_tx.block_number, current_state.expected_block_number,
            );
            return Vec::new();
        }

        let mut result = Vec::with_capacity(n_txs);
        while result.len() < n_txs {
            let Some(front_tx) = self.queue.front() else {
                break;
            };
            if !current_state.matches_tx(front_tx) {
                break;
            }

            let tx = self.queue.pop_front().expect("Queue front must exist if peek succeeded");
            result.push(tx.tx_reference);
            self.staged_txs.push(tx);
        }

        debug!(
            "FIFO pop_ready_chunk: popped {} txs (requested cap n_txs={}, queue_len_after={})",
            result.len(),
            n_txs,
            self.queue.len()
        );

        if !result.is_empty() {
            current_state.advance_block_if_drained(&self.queue);
        }
        result
    }

    // Returns true if at least one transaction of this address was removed from the queue or from
    // staged transactions.
    fn remove_by_address(&mut self, address: ContractAddress) -> bool {
        let len_before = self.queue.len() + self.staged_txs.len();
        self.queue.retain(|tx| tx.tx_reference.address != address);
        self.staged_txs.retain(|tx| tx.tx_reference.address != address);
        let len_after = self.queue.len() + self.staged_txs.len();
        len_before != len_after
    }

    fn remove_txs(&mut self, txs: &[TransactionReference]) -> Vec<TransactionReference> {
        let mut tx_hashes: HashSet<TransactionHash> = txs.iter().map(|tx| tx.tx_hash).collect();
        let mut removed_hashes: HashSet<TransactionHash> = HashSet::with_capacity(tx_hashes.len());
        self.queue.retain(|tx| {
            let tx_hash = tx.tx_reference.tx_hash;
            if tx_hashes.remove(&tx_hash) {
                removed_hashes.insert(tx_hash);
                false
            } else {
                true
            }
        });
        self.staged_txs.retain(|tx| {
            let tx_hash = tx.tx_reference.tx_hash;
            if tx_hashes.remove(&tx_hash) {
                removed_hashes.insert(tx_hash);
                false
            } else {
                true
            }
        });
        txs.iter().copied().filter(|tx| removed_hashes.contains(&tx.tx_hash)).collect()
    }

    fn has_ready_txs(&self) -> bool {
        match (self.current_proposal_state, self.queue.front()) {
            (Some(state), Some(front_tx)) => state.matches_tx(front_tx),
            _ => false,
        }
    }

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_> {
        let Some(state) = self.current_proposal_state else {
            return Box::new(std::iter::empty());
        };

        Box::new(
            self.queue.iter().take_while(move |tx| state.matches_tx(tx)).map(|tx| &tx.tx_reference),
        )
    }

    fn queue_snapshot(&self) -> TransactionQueueSnapshot {
        // FIFO queue doesn't have priority/pending distinction.
        let priority_queue: Vec<TransactionHash> =
            self.queue.iter().map(|tx| tx.tx_reference.tx_hash).collect();
        TransactionQueueSnapshot {
            gas_price_threshold: GasPrice::default(),
            priority_queue,
            pending_queue: Vec::new(),
        }
    }

    fn rewind_txs(&mut self, rewind_data: RewindData<'_>) -> IndexSet<TransactionHash> {
        // Extract FIFO-specific data
        let RewindData::Fifo { committed_nonces, rejected_tx_hashes } = rewind_data else {
            unreachable!("FifoTransactionQueue received FeePriority data instead of Fifo data");
        };

        let txs_to_rewind = self.collect_txs_to_rewind(committed_nonces, rejected_tx_hashes);
        let rewound_hashes: IndexSet<TransactionHash> = txs_to_rewind
            .into_iter()
            // We push each rewound tx to the FRONT, so iterate in reverse to preserve original
            // FIFO order among rewound transactions.
            .rev()
            .map(|tx| {
                debug!(
                    "FIFO rewind: tx_hash={}, timestamp={}, block_number={}, queue_before={:?}",
                    tx.tx_reference.tx_hash, tx.timestamp, tx.block_number, self.queue
                );
                InsertionSide::Front.push(&mut self.queue, tx);
                tx.tx_reference.tx_hash
            })
            .collect();

        self.staged_txs.clear();

        rewound_hashes
    }

    fn priority_queue_len(&self) -> usize {
        self.queue.len()
    }

    fn pending_queue_len(&self) -> usize {
        0
    }

    fn resolve_metadata(&mut self) -> BlockMetadata {
        if self.queue.front().is_some() {
            return self.sync_proposal_state_from_queue_front_tx();
        }
        // Queue is empty: reuse previous timestamp and block number if they exist.
        match self.current_proposal_state {
            Some(state) => {
                debug!(
                    "FIFO resolve_metadata: queue empty, reusing last_timestamp={:?}, \
                     expected_block={:?}",
                    state.timestamp, state.expected_block_number
                );
                BlockMetadata { timestamp: state.timestamp, block_number: Some(state.expected_block_number) }
            }
            None => {
                debug!("FIFO resolve_metadata: queue empty, no previous proposal state");
                BlockMetadata { timestamp: 0, block_number: None }
            }
        }
    }

    fn update_tx_block_metadata(&mut self, tx_hash: TransactionHash, metadata: TxBlockMetadata) {
        self.pending_metadata.insert(tx_hash, metadata);
        assert!(
            self.pending_metadata.len() <= 1000,
            "FIFO pending_metadata unexpectedly large: {}. Metadata should be removed once the tx \
             is added to the queue.",
            self.pending_metadata.len()
        );
    }
}
