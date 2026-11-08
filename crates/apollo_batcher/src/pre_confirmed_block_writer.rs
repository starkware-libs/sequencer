use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_config::config::PreconfirmedBlockWriterConfig;
use apollo_batcher_types::batcher_types::Round;
use apollo_starknet_client::reader::StateDiff;
use async_trait::async_trait;
use futures::future::{BoxFuture, OptionFuture};
use futures::FutureExt;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::cende_client_types::{
    CendeBlockMetadata,
    CendePreconfirmedBlock,
    CendePreconfirmedTransaction,
    StarknetClientTransactionReceipt,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    PreconfirmedCendeClientTrait,
};

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

/// Coordinates the flow of pre-confirmed block data during block proposal.
/// Listens for transaction updates from the block builder via dedicated channels and utilizes a
/// Cende client to communicate the updates to the Cende recorder.
/// Closing the transaction channel flushes everything received so far; cancelling the writer ends
/// it without that final write, so a proposal that failed to build never reaches the feed.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait PreconfirmedBlockWriterTrait: Send {
    async fn run(&mut self);
}

pub struct PreconfirmedBlockWriter {
    pre_confirmed_block_writer_input: PreconfirmedBlockWriterInput,
    pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
    cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
    write_block_interval_millis: u64,
    cancellation_token: CancellationToken,
}

impl PreconfirmedBlockWriter {
    pub fn new(
        pre_confirmed_block_writer_input: PreconfirmedBlockWriterInput,
        pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
        cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
        write_block_interval_millis: u64,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            pre_confirmed_block_writer_input,
            pre_confirmed_tx_receiver,
            cende_client,
            write_block_interval_millis,
            cancellation_token,
        }
    }

    fn create_pre_confirmed_block(
        &self,
        transactions_map: &IndexMap<
            TransactionHash,
            (CendePreconfirmedTransaction, StarknetClientTransactionReceipt, StateDiff),
        >,
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

    /// The last write of the block. An in-loop write that fails is resent on a later tick; no
    /// tick follows this one, so it retries by itself, up to `MAX_FINAL_WRITE_ATTEMPTS` times.
    async fn final_write(
        &self,
        transactions_map: &IndexMap<
            TransactionHash,
            (CendePreconfirmedTransaction, StarknetClientTransactionReceipt, StateDiff),
        >,
        mut write_iteration: u64,
    ) {
        for attempt in 1..=MAX_FINAL_WRITE_ATTEMPTS {
            let pre_confirmed_block =
                self.create_pre_confirmed_block(transactions_map, write_iteration);
            let cende_client = Arc::clone(&self.cende_client);
            let outcome =
                best_effort_write_pre_confirmed_block(cende_client, pre_confirmed_block).await;
            if !outcome.needs_resend() || attempt == MAX_FINAL_WRITE_ATTEMPTS {
                return;
            }
            write_iteration += 1;
            tokio::time::sleep(retry_backoff(attempt)).await;
        }
    }
}

#[async_trait]
impl PreconfirmedBlockWriterTrait for PreconfirmedBlockWriter {
    async fn run(&mut self) {
        let mut transactions_map: IndexMap<
            TransactionHash,
            (CendePreconfirmedTransaction, StarknetClientTransactionReceipt, StateDiff),
        > = IndexMap::new();

        let mut pending_write_task: Option<BoxFuture<'static, WriteOutcome>> = None;
        let mut write_pre_confirmed_txs_timer =
            tokio::time::interval(Duration::from_millis(self.write_block_interval_millis));
        let mut retry_state = RetryState::default();

        // We initially mark that we have pending changes so that the client will write to the
        // Cende recorder that a new proposal round has started.
        let mut pending_changes = true;
        let mut next_write_iteration = 0;
        let mut cancelled = pin!(self.cancellation_token.cancelled());

        loop {
            tokio::select! {
                biased;
                () = &mut cancelled => {
                    info!(
                        "Pre confirmed block writer cancelled, dropping {} buffered transactions \
                         without a final write",
                        transactions_map.len()
                    );
                    return;
                }
                _ = write_pre_confirmed_txs_timer.tick() => {
                    // TODO(noamsp): Extract to a function.
                    // Only send if there are pending changes to avoid unnecessary calls
                    if pending_changes && retry_state.write_allowed() {
                        // Check if there are any ongoing write tasks to avoid contention
                        if pending_write_task.is_none() {
                            let pre_confirmed_block = self.create_pre_confirmed_block(
                                &transactions_map,
                                next_write_iteration,
                            );
                            let cende_client = Arc::clone(&self.cende_client);
                            pending_write_task = Some(Box::pin(async move {
                                best_effort_write_pre_confirmed_block(cende_client, pre_confirmed_block).await
                            }));
                            next_write_iteration += 1;
                            pending_changes = false;
                        } else {
                            debug!("Waiting another write cycle because write iteration {} is still ongoing", next_write_iteration - 1);
                        }
                    }
                }

                Some(outcome) = OptionFuture::from(pending_write_task.as_mut()) => {
                    pending_write_task = None;
                    pending_changes |= retry_state.should_resend_after(outcome);
                }
                msg = self.pre_confirmed_tx_receiver.recv() => {
                    match msg {
                        Some((tx, tx_receipt, tx_state_diff)) => {
                            let tx = CendePreconfirmedTransaction::from(tx);
                            let tx_hash = tx.transaction_hash();
                            transactions_map.insert(tx_hash, (tx, tx_receipt, tx_state_diff));
                            pending_changes = true;
                        }
                        None => {
                            info!("Pre confirmed tx channel closed");
                            break;
                        }
                    }
                }
            }
        }

        // A write still in flight may be stuck on its connection. The final write carries every
        // transaction with a higher iteration, so it supersedes the dropped write.
        if let Some(task) = pending_write_task.take() {
            match task.now_or_never() {
                Some(outcome) => pending_changes |= retry_state.should_resend_after(outcome),
                None => pending_changes = true,
            }
        }

        if pending_changes {
            self.final_write(&transactions_map, next_write_iteration).await;
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
    ) -> (Box<dyn PreconfirmedBlockWriterTrait>, PreconfirmedTxSender, CancellationToken);
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
    ) -> (Box<dyn PreconfirmedBlockWriterTrait>, PreconfirmedTxSender, CancellationToken) {
        info!("Create pre confirmed block writer for block {block_number}, round {round}");
        let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);
        let cancellation_token = CancellationToken::new();

        let cende_client = self.cende_client.clone();

        let pre_confirmed_block_writer_input =
            PreconfirmedBlockWriterInput { block_number, round, block_metadata };

        let pre_confirmed_block_writer = Box::new(PreconfirmedBlockWriter::new(
            pre_confirmed_block_writer_input,
            pre_confirmed_tx_receiver,
            cende_client,
            self.config.write_block_interval_millis,
            cancellation_token.clone(),
        ));
        (pre_confirmed_block_writer, pre_confirmed_tx_sender, cancellation_token)
    }
}

// TODO(noamsp): find a better name for this struct.
pub struct PreconfirmedBlockWriterInput {
    pub block_number: BlockNumber,
    pub round: Round,
    pub block_metadata: CendeBlockMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WriteOutcome {
    Written,
    /// The recorder refused the write; resending the same block cannot succeed.
    Rejected,
    Failed,
}

impl WriteOutcome {
    fn needs_resend(self) -> bool {
        match self {
            Self::Written | Self::Rejected => false,
            Self::Failed => true,
        }
    }
}

const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(50);
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(1);
pub(crate) const MAX_FINAL_WRITE_ATTEMPTS: u32 = 3;

pub(crate) fn retry_backoff(consecutive_failures: u32) -> Duration {
    let doublings = consecutive_failures.saturating_sub(1);
    INITIAL_RETRY_BACKOFF.saturating_mul(2u32.saturating_pow(doublings)).min(MAX_RETRY_BACKOFF)
}

#[derive(Default)]
pub(crate) struct RetryState {
    consecutive_failures: u32,
    next_write_allowed_at: Option<tokio::time::Instant>,
}

impl RetryState {
    pub(crate) fn should_resend_after(&mut self, outcome: WriteOutcome) -> bool {
        if !outcome.needs_resend() {
            *self = Self::default();
            return false;
        }
        self.consecutive_failures += 1;
        self.next_write_allowed_at =
            Some(tokio::time::Instant::now() + retry_backoff(self.consecutive_failures));
        true
    }

    pub(crate) fn write_allowed(&self) -> bool {
        self.next_write_allowed_at
            .is_none_or(|allowed_at| tokio::time::Instant::now() >= allowed_at)
    }
}

/// Each write is best effort: errors are never propagated, only mapped to a `WriteOutcome` so the
/// caller can decide whether to resend.
async fn best_effort_write_pre_confirmed_block(
    cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
    pre_confirmed_block: CendeWritePreconfirmedBlock,
) -> WriteOutcome {
    match cende_client.write_pre_confirmed_block(pre_confirmed_block).await {
        Ok(()) => WriteOutcome::Written,
        Err(error) if error.is_retryable() => WriteOutcome::Failed,
        Err(_) => WriteOutcome::Rejected,
    }
}
