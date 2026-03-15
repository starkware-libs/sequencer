use std::collections::{HashMap, HashSet, VecDeque};

use apollo_mempool_types::mempool_types::{TransactionQueueSnapshot, TxBlockMetadata};
use indexmap::IndexSet;
use starknet_api::block::{BlockNumber, GasPrice, UnixTimestamp};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;
use tracing::debug;

use crate::mempool::TransactionReference;
use crate::transaction_queue_trait::{RewindData, TransactionQueueTrait};

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
    fn front_queued_tx_matches(&self, front_queued_tx: &FifoTransaction) -> bool {
        !self.emit_empty_block
            && front_queued_tx.timestamp == self.timestamp
            && front_queued_tx.block_number == self.expected_block_number
    }

    fn advance_expected_block_after_pop(&mut self) {
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
    // Tracks current proposal metadata in FIFO mode:
    // - timestamp returned by resolve_timestamp()
    // - expected block number for get_txs()
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

    fn update_proposal_state(&mut self, front_queued_tx: FifoTransaction) {
        // If this is the first proposal in this queue, start directly from the front tx block.
        let Some(prev_state) = self.current_proposal_state else {
            self.current_proposal_state = Some(CurrentProposalState {
                timestamp: front_queued_tx.timestamp,
                expected_block_number: front_queued_tx.block_number,
                emit_empty_block: false,
            });
            return;
        };

        // front < expected: rewind put older txs back in front; re-anchor expected block.
        // front > expected: there is a gap; emit one empty block for this proposal.
        // front == expected: regular proposal, no gap.
        let (expected_block_number, emit_empty_block) =
            if front_queued_tx.block_number < prev_state.expected_block_number {
                (front_queued_tx.block_number, false)
            } else if front_queued_tx.block_number > prev_state.expected_block_number {
                (
                    prev_state
                        .expected_block_number
                        .next()
                        .expect("Block number overflow while advancing expected block."),
                    true,
                )
            } else {
                (prev_state.expected_block_number, false)
            };

        self.current_proposal_state = Some(CurrentProposalState {
            timestamp: front_queued_tx.timestamp,
            expected_block_number,
            emit_empty_block,
        });
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
        let Some(mut current_state) = self.current_proposal_state else {
            // Defensive path: get_txs called without resolve_timestamp first.
            return Vec::new();
        };
        let Some(front_queued_tx) = self.queue.front().copied() else {
            // Queue is empty, nothing to return.
            return Vec::new();
        };
        if !current_state.front_queued_tx_matches(&front_queued_tx) {
            debug!("FIFO pop_ready_chunk: returning empty block");
            return Vec::new();
        }

        let mut result = Vec::with_capacity(n_txs);
        while result.len() < n_txs {
            let Some(front_tx) = self.queue.front() else {
                break;
            };
            if !current_state.front_queued_tx_matches(front_tx) {
                break;
            }

            let tx = self.queue.pop_front().expect("Queue front must exist if peek succeeded");
            result.push(tx.tx_reference);
            self.staged_txs.push(tx);
        }
        if !result.is_empty() {
            // Only advance expected block when we actually popped txs for the current block.
            current_state.advance_expected_block_after_pop();
            self.current_proposal_state = Some(current_state);
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
        self.current_proposal_state
            .zip(self.queue.front())
            .is_some_and(|(state, front_queued_tx)| state.front_queued_tx_matches(front_queued_tx))
    }

    fn iter_over_ready_txs(&self) -> Box<dyn Iterator<Item = &TransactionReference> + '_> {
        Box::new(self.queue.iter().map(|tx| &tx.tx_reference))
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

    fn resolve_timestamp(&mut self) -> UnixTimestamp {
        // For each proposal, derive timestamp/expected-block from queue front and current state.
        if let Some(&front_tx) = self.queue.front() {
            self.update_proposal_state(front_tx);
            return front_tx.timestamp;
        }
        // Queue is empty: reuse previous timestamp if it exists, otherwise return 0.
        match self.current_proposal_state {
            Some(state) => {
                debug!(
                    "FIFO resolve_timestamp: queue empty, reusing last_timestamp={:?}, \
                     expected_block={:?}",
                    state.timestamp, state.expected_block_number
                );
                state.timestamp
            }
            None => {
                debug!("FIFO resolve_timestamp: queue empty, no previous proposal state");
                0
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
