use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_config::config::PreconfirmedBlockWriterConfig;
use apollo_batcher_types::batcher_types::Round;
use apollo_starknet_client::reader::StateDiff;
use async_trait::async_trait;
use futures::future::BoxFuture;
use indexmap::map::Entry;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tokio::time::{Instant, Sleep};
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
    PreconfirmedCendeClientResult,
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

    fn start_write_if_needed(
        &self,
        transactions_map: &TransactionsMap,
        pending_write_task: &mut Option<BoxFuture<'static, PreconfirmedCendeClientResult<()>>>,
        needs_flush: &mut bool,
        next_write_iteration: &mut u64,
        next_allowed_write_at: &mut Instant,
        now: Instant,
    ) -> bool {
        if !*needs_flush || pending_write_task.is_some() || now < *next_allowed_write_at {
            return false;
        }

        let pre_confirmed_block =
            self.create_pre_confirmed_block(transactions_map, *next_write_iteration);
        let cende_client = Arc::clone(&self.cende_client);
        *pending_write_task = Some(Box::pin(async move {
            cende_client.write_pre_confirmed_block(pre_confirmed_block).await
        }));
        *next_write_iteration += 1;
        *needs_flush = false;
        *next_allowed_write_at = now + Duration::from_millis(self.write_block_interval_millis);
        true
    }

    fn arm_throttle_if_needed(
        pending_write_task: &Option<BoxFuture<'static, PreconfirmedCendeClientResult<()>>>,
        needs_flush: bool,
        next_allowed_write_at: Instant,
        throttle_sleep: &mut Option<Pin<Box<Sleep>>>,
        now: Instant,
    ) {
        if needs_flush && pending_write_task.is_none() && now < next_allowed_write_at {
            *throttle_sleep = Some(Box::pin(tokio::time::sleep_until(next_allowed_write_at)));
        } else {
            *throttle_sleep = None;
        }
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
        let mut transactions_map: TransactionsMap = IndexMap::new();

        let mut pending_write_task: Option<BoxFuture<'static, PreconfirmedCendeClientResult<()>>> =
            None;
        let mut needs_flush = true;
        let mut next_write_iteration = 0;
        let mut next_allowed_write_at = Instant::now();
        let mut throttle_sleep: Option<Pin<Box<Sleep>>> = None;
        let mut should_stop = false;

        let now = Instant::now();
        self.start_write_if_needed(
            &transactions_map,
            &mut pending_write_task,
            &mut needs_flush,
            &mut next_write_iteration,
            &mut next_allowed_write_at,
            now,
        );
        Self::arm_throttle_if_needed(
            &pending_write_task,
            needs_flush,
            next_allowed_write_at,
            &mut throttle_sleep,
            now,
        );

        loop {
            tokio::select! {
                result = async {
                    match pending_write_task.as_mut() {
                        Some(task) => task.await,
                        None => std::future::pending::<PreconfirmedCendeClientResult<()>>().await,
                    }
                } => {
                    pending_write_task = None;
                    if result.is_err() {
                        // Intentionally ignore write-task errors (including recorder BAD_REQUEST)
                        // to preserve current best-effort pre-confirmed block writer behavior.
                    }
                    let now = Instant::now();
                    self.start_write_if_needed(
                        &transactions_map,
                        &mut pending_write_task,
                        &mut needs_flush,
                        &mut next_write_iteration,
                        &mut next_allowed_write_at,
                        now,
                    );
                    Self::arm_throttle_if_needed(
                        &pending_write_task,
                        needs_flush,
                        next_allowed_write_at,
                        &mut throttle_sleep,
                        now,
                    );
                }
                _ = async {
                    match throttle_sleep.as_mut() {
                        Some(sleep) => sleep.as_mut().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    throttle_sleep = None;
                    let now = Instant::now();
                    self.start_write_if_needed(
                        &transactions_map,
                        &mut pending_write_task,
                        &mut needs_flush,
                        &mut next_write_iteration,
                        &mut next_allowed_write_at,
                        now,
                    );
                    Self::arm_throttle_if_needed(
                        &pending_write_task,
                        needs_flush,
                        next_allowed_write_at,
                        &mut throttle_sleep,
                        now,
                    );
                }
                msg = self.pre_confirmed_tx_receiver.recv() => {
                    match msg {
                        Some((tx, tx_receipt, tx_state_diff)) => {
                            if Self::apply_preconfirmed_update(
                                &mut transactions_map,
                                tx,
                                tx_receipt,
                                tx_state_diff,
                            ) {
                                needs_flush = true;
                            }
                            let now = Instant::now();
                            self.start_write_if_needed(
                                &transactions_map,
                                &mut pending_write_task,
                                &mut needs_flush,
                                &mut next_write_iteration,
                                &mut next_allowed_write_at,
                                now,
                            );
                            Self::arm_throttle_if_needed(
                                &pending_write_task,
                                needs_flush,
                                next_allowed_write_at,
                                &mut throttle_sleep,
                                now,
                            );
                        }
                        None => {
                            info!("Pre confirmed tx channel closed");
                            should_stop = true;
                        }
                    }
                }
                msg = self.candidate_tx_receiver.recv() => {
                    match msg {
                        Some(txs) => {
                            if Self::apply_candidate_updates(&mut transactions_map, txs) {
                                needs_flush = true;
                            }
                            let now = Instant::now();
                            self.start_write_if_needed(
                                &transactions_map,
                                &mut pending_write_task,
                                &mut needs_flush,
                                &mut next_write_iteration,
                                &mut next_allowed_write_at,
                                now,
                            );
                            Self::arm_throttle_if_needed(
                                &pending_write_task,
                                needs_flush,
                                next_allowed_write_at,
                                &mut throttle_sleep,
                                now,
                            );
                        }
                        None => {
                            info!("Candidate tx channel closed");
                            should_stop = true;
                        }
                    }
                }
            }

            if should_stop {
                break;
            }
        }

        // Wait for the pending write task to complete gracefully.
        if let Some(task) = pending_write_task.take() {
            let result: PreconfirmedCendeClientResult<()> = task.await;
            if result.is_err() {
                // Intentionally ignore write-task errors (including recorder BAD_REQUEST)
                // to preserve current best-effort pre-confirmed block writer behavior.
            }
        }

        if needs_flush {
            let now = Instant::now();
            if now < next_allowed_write_at {
                tokio::time::sleep_until(next_allowed_write_at).await;
            }
            let pre_confirmed_block =
                self.create_pre_confirmed_block(&transactions_map, next_write_iteration);
            let result = self.cende_client.write_pre_confirmed_block(pre_confirmed_block).await;
            if result.is_err() {
                // Intentionally ignore write-task errors (including recorder BAD_REQUEST)
                // to preserve current best-effort pre-confirmed block writer behavior.
            }
        }
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
