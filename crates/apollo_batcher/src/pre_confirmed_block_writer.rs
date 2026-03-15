use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_config::config::PreconfirmedBlockWriterConfig;
use apollo_batcher_types::batcher_types::Round;
use apollo_starknet_client::reader::StateDiff;
use async_trait::async_trait;
use indexmap::map::Entry;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::info;

use crate::cende_client_types::{
    CendeBlockMetadata,
    CendePreconfirmedBlock,
    CendePreconfirmedTransaction,
    StarknetClientTransactionReceipt,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    PreconfirmedCendeClientError,
    PreconfirmedCendeClientTrait,
};

#[derive(Debug, Error)]
pub enum BlockWriterError {
    #[error(transparent)]
    PreconfirmedCendeClientError(#[from] PreconfirmedCendeClientError),
}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;

pub type CandidateTxReceiver = tokio::sync::mpsc::Receiver<Vec<InternalConsensusTransaction>>;
pub type CandidateTxSender = tokio::sync::mpsc::Sender<Vec<InternalConsensusTransaction>>;

pub type PreconfirmedTxReceiver = tokio::sync::mpsc::Receiver<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    StateDiff,
)>;

pub type PreconfirmedTxSender = tokio::sync::mpsc::Sender<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    StateDiff,
)>;

type TransactionsMap = IndexMap<
    TransactionHash,
    (CendePreconfirmedTransaction, Option<StarknetClientTransactionReceipt>, Option<StateDiff>),
>;

#[derive(Default)]
struct SharedWriterState {
    transactions_map: TransactionsMap,
    dirty: bool,
    closed: bool,
}

/// Coordinates the flow of pre-confirmed block data during block proposal.
/// Listens for transaction updates from the block builder via dedicated channels and utilizes a
/// Cende client to communicate the updates to the Cende recorder.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait PreconfirmedBlockWriterTrait: Send {
    async fn run(&mut self);
}

pub struct PreconfirmedBlockWriter {
    pre_confirmed_block_writer_input: PreconfirmedBlockWriterInput,
    candidate_tx_receiver: CandidateTxReceiver,
    pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
    cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
    write_block_interval_millis: u64,
}

impl PreconfirmedBlockWriter {
    pub fn new(
        pre_confirmed_block_writer_input: PreconfirmedBlockWriterInput,
        candidate_tx_receiver: CandidateTxReceiver,
        pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
        cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
        write_block_interval_millis: u64,
    ) -> Self {
        Self {
            pre_confirmed_block_writer_input,
            candidate_tx_receiver,
            pre_confirmed_tx_receiver,
            cende_client,
            write_block_interval_millis,
        }
    }

    fn apply_preconfirmed_update(
        transactions_map: &mut TransactionsMap,
        tx: InternalConsensusTransaction,
        tx_receipt: StarknetClientTransactionReceipt,
        tx_state_diff: StateDiff,
    ) -> bool {
        let tx = CendePreconfirmedTransaction::from(tx);
        let tx_hash = tx.transaction_hash();
        transactions_map.insert(tx_hash, (tx, Some(tx_receipt), Some(tx_state_diff)));
        true
    }

    fn apply_candidate_updates(
        transactions_map: &mut TransactionsMap,
        txs: Vec<InternalConsensusTransaction>,
    ) -> bool {
        let mut has_changes = false;
        // Skip transactions that were already executed, to avoid an unnecessary write.
        for tx in txs {
            let tx = CendePreconfirmedTransaction::from(tx);
            match transactions_map.entry(tx.transaction_hash()) {
                Entry::Vacant(entry) => {
                    entry.insert((tx, None, None));
                    has_changes = true;
                }
                Entry::Occupied(_) => {}
            }
        }
        has_changes
    }

    fn create_pre_confirmed_block(
        &self,
        transactions_map: &TransactionsMap,
        write_iteration: u64,
    ) -> CendeWritePreconfirmedBlock {
        let mut transactions = Vec::with_capacity(transactions_map.len());
        let mut transaction_receipts = Vec::with_capacity(transactions_map.len());
        let mut transaction_state_diffs = Vec::with_capacity(transactions_map.len());

        for (tx, tx_receipt, tx_state_diff) in transactions_map.values() {
            transactions.push(tx.clone());
            transaction_receipts.push(tx_receipt.clone());
            transaction_state_diffs.push(tx_state_diff.clone());
        }

        let pre_confirmed_block = CendePreconfirmedBlock {
            metadata: self.pre_confirmed_block_writer_input.block_metadata.clone(),
            transactions,
            transaction_receipts,
            transaction_state_diffs,
        };

        CendeWritePreconfirmedBlock {
            block_number: self.pre_confirmed_block_writer_input.block_number,
            round: self.pre_confirmed_block_writer_input.round,
            write_iteration,
            pre_confirmed_block,
        }
    }
}

#[async_trait]
impl PreconfirmedBlockWriterTrait for PreconfirmedBlockWriter {
    async fn run(&mut self) {
        let shared_state = Arc::new(tokio::sync::Mutex::new(SharedWriterState::default()));
        let shutdown_notify = Arc::new(tokio::sync::Notify::new());

        let (_unused_candidate_sender, empty_candidate_receiver) = tokio::sync::mpsc::channel(1);
        let (_unused_preconfirmed_sender, empty_preconfirmed_receiver) =
            tokio::sync::mpsc::channel(1);
        let mut candidate_rx =
            std::mem::replace(&mut self.candidate_tx_receiver, empty_candidate_receiver);
        let mut preconfirmed_rx =
            std::mem::replace(&mut self.pre_confirmed_tx_receiver, empty_preconfirmed_receiver);

        let receiver_state = Arc::clone(&shared_state);
        let receiver_notify = Arc::clone(&shutdown_notify);
        let receiver_loop = async move {
            loop {
                tokio::select! {
                    msg = preconfirmed_rx.recv() => {
                        match msg {
                            Some((tx, tx_receipt, tx_state_diff)) => {
                                let mut state = receiver_state.lock().await;
                                if Self::apply_preconfirmed_update(
                                    &mut state.transactions_map,
                                    tx,
                                    tx_receipt,
                                    tx_state_diff,
                                ) {
                                    state.dirty = true;
                                }
                            }
                            None => {
                                info!("Pre confirmed tx channel closed");
                                let mut state = receiver_state.lock().await;
                                state.closed = true;
                                receiver_notify.notify_waiters();
                                break;
                            }
                        }
                    }
                    msg = candidate_rx.recv() => {
                        match msg {
                            Some(txs) => {
                                let mut state = receiver_state.lock().await;
                                if Self::apply_candidate_updates(&mut state.transactions_map, txs) {
                                    state.dirty = true;
                                }
                            }
                            None => {
                                info!("Candidate tx channel closed");
                                let mut state = receiver_state.lock().await;
                                state.closed = true;
                                receiver_notify.notify_waiters();
                                break;
                            }
                        }
                    }
                }
            }
        };

        let writer_state = Arc::clone(&shared_state);
        let writer_notify = Arc::clone(&shutdown_notify);
        let writer_interval_millis = self.write_block_interval_millis;
        let writer_loop = async {
            let mut next_write_iteration = 0;
            let mut interval = tokio::time::interval(Duration::from_millis(writer_interval_millis));
            let mut last_write_started_at: Option<std::time::Instant> = None;

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = writer_notify.notified() => {}
                }

                let (dirty, closed, snapshot) = {
                    let mut state = writer_state.lock().await;
                    let dirty = state.dirty;
                    let closed = state.closed;
                    let snapshot = if dirty {
                        state.dirty = false;
                        Some(state.transactions_map.clone())
                    } else {
                        None
                    };
                    (dirty, closed, snapshot)
                };

                if let Some(snapshot) = snapshot {
                    if let Some(last_started_at) = last_write_started_at {
                        let elapsed = last_started_at.elapsed();
                        let interval_duration = Duration::from_millis(writer_interval_millis);
                        if elapsed < interval_duration {
                            tokio::time::sleep(interval_duration - elapsed).await;
                        }
                    }

                    let pre_confirmed_block =
                        self.create_pre_confirmed_block(&snapshot, next_write_iteration);
                    last_write_started_at = Some(std::time::Instant::now());
                    next_write_iteration += 1;
                    let result =
                        self.cende_client.write_pre_confirmed_block(pre_confirmed_block).await;
                    if result.is_err() {
                        // Intentionally ignore write errors (including recorder BAD_REQUEST)
                        // to preserve current best-effort pre-confirmed block writer behavior.
                    }
                }

                if closed {
                    if !dirty {
                        break;
                    }
                    continue;
                }
            }
        };

        let _ = tokio::join!(receiver_loop, writer_loop);
        info!("Pre confirmed block writer finished");
    }
}

#[cfg_attr(test, automock)]
pub trait PreconfirmedBlockWriterFactoryTrait: Send + Sync {
    fn create(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        block_metadata: CendeBlockMetadata,
    ) -> (Box<dyn PreconfirmedBlockWriterTrait>, CandidateTxSender, PreconfirmedTxSender);
}

pub struct PreconfirmedBlockWriterFactory {
    pub config: PreconfirmedBlockWriterConfig,
    pub cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
}

impl PreconfirmedBlockWriterFactoryTrait for PreconfirmedBlockWriterFactory {
    fn create(
        &self,
        block_number: BlockNumber,
        round: Round,
        block_metadata: CendeBlockMetadata,
    ) -> (Box<dyn PreconfirmedBlockWriterTrait>, CandidateTxSender, PreconfirmedTxSender) {
        info!("Create pre confirmed block writer for block {block_number}, round {round}");
        // Initialize channels for communication between the pre confirmed block writer and the
        // block builder.
        let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);
        let (candidate_tx_sender, candidate_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);

        let cende_client = self.cende_client.clone();

        let pre_confirmed_block_writer_input =
            PreconfirmedBlockWriterInput { block_number, round, block_metadata };

        let pre_confirmed_block_writer: Box<dyn PreconfirmedBlockWriterTrait> =
            Box::new(PreconfirmedBlockWriter::new(
                pre_confirmed_block_writer_input,
                candidate_tx_receiver,
                pre_confirmed_tx_receiver,
                cende_client,
                self.config.write_block_interval_millis,
            ));
        (pre_confirmed_block_writer, candidate_tx_sender, pre_confirmed_tx_sender)
    }
}

// TODO(noamsp): find a better name for this struct.
pub struct PreconfirmedBlockWriterInput {
    pub block_number: BlockNumber,
    pub round: Round,
    pub block_metadata: CendeBlockMetadata,
}
