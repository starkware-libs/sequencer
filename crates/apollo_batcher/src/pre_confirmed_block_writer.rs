use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::state::ThinStateDiff;
use thiserror::Error;
use tracing::info;

use crate::cende_client_types::{
    CendeBlockMetadata,
    CendePreConfirmedBlock,
    CendePreConfirmedTransaction,
    StarknetClientTransactionReceipt,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreConfirmedBlock,
    PreConfirmedCendeClientError,
    PreConfirmedCendeClientTrait,
};

#[derive(Debug, Error)]
pub enum BlockWriterError {
    #[error(transparent)]
    PreConfirmedCendeClientError(#[from] PreConfirmedCendeClientError),
}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;

pub type PreConfirmedTxReceiver = tokio::sync::mpsc::Receiver<Vec<InternalConsensusTransaction>>;
pub type PreConfirmedTxSender = tokio::sync::mpsc::Sender<Vec<InternalConsensusTransaction>>;

pub type ExecutedTxReceiver = tokio::sync::mpsc::Receiver<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    ThinStateDiff,
)>;
pub type ExecutedTxSender = tokio::sync::mpsc::Sender<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    ThinStateDiff,
)>;

/// Coordinates the flow of pre-confirmed block data during block proposal.
/// Listens for transaction updates from the block builder via dedicated channels and utilizes a
/// Cende client to communicate the updates to the Cende recorder.
#[async_trait]
#[cfg_attr(test, automock)]
pub trait PreConfirmedBlockWriterTrait: Send {
    async fn run(&mut self) -> BlockWriterResult<()>;
}

pub struct PreConfirmedBlockWriter {
    pre_confirmed_block_writer_input: PreConfirmedBlockWriterInput,
    pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
    executed_tx_receiver: ExecutedTxReceiver,
    cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
}

impl PreConfirmedBlockWriter {
    pub fn new(
        pre_confirmed_block_writer_input: PreConfirmedBlockWriterInput,
        pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
        executed_tx_receiver: ExecutedTxReceiver,
        cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
    ) -> Self {
        Self {
            pre_confirmed_block_writer_input,
            pre_confirmed_tx_receiver,
            executed_tx_receiver,
            cende_client,
        }
    }

    fn create_pre_confirmed_block(
        &self,
        transactions_map: &IndexMap<
            CendePreConfirmedTransaction,
            (Option<StarknetClientTransactionReceipt>, Option<ThinStateDiff>),
        >,
        write_iteration: u64,
    ) -> CendeWritePreConfirmedBlock {
        let mut transactions = Vec::with_capacity(transactions_map.len());
        let mut transaction_receipts = Vec::with_capacity(transactions_map.len());
        let mut transaction_state_diffs = Vec::with_capacity(transactions_map.len());

        for (tx, (tx_receipt, tx_state_diff)) in transactions_map {
            transactions.push(tx.clone());
            transaction_receipts.push(tx_receipt.clone());
            transaction_state_diffs.push(tx_state_diff.clone());
        }

        let pre_confirmed_block = CendePreConfirmedBlock {
            metadata: self.pre_confirmed_block_writer_input.block_metadata.clone(),
            transactions,
            transaction_receipts,
            transaction_state_diffs,
        };

        CendeWritePreConfirmedBlock {
            block_number: self.pre_confirmed_block_writer_input.block_number,
            round: self.pre_confirmed_block_writer_input.round,
            write_iteration,
            pre_confirmed_block,
        }
    }
}

#[async_trait]
impl PreConfirmedBlockWriterTrait for PreConfirmedBlockWriter {
    async fn run(&mut self) -> BlockWriterResult<()> {
        let mut transactions_map: IndexMap<
            CendePreConfirmedTransaction,
            (Option<StarknetClientTransactionReceipt>, Option<ThinStateDiff>),
        > = IndexMap::new();

        let mut pending_tasks = FuturesUnordered::new();
        // TODO(noamsp): make this configurable.
        let mut write_executed_txs_timer = tokio::time::interval(Duration::from_millis(50));

        // We initially mark that we have pending changes so that the client will write to the
        // Cende recorder that a new proposal round has started.
        let mut pending_changes = true;
        let mut write_iteration: u64 = 0;

        loop {
            tokio::select! {
                _ = write_executed_txs_timer.tick() => {
                    // Only send if there are pending changes to avoid unnecessary calls
                    if pending_changes {
                        let pre_confirmed_block = self.create_pre_confirmed_block(
                            &transactions_map,
                            write_iteration,
                        );
                        pending_tasks.push(self.cende_client.write_pre_confirmed_block(pre_confirmed_block));
                        write_iteration += 1;
                        pending_changes = false;
                    }
                }
                // TODO(noamsp): Handle height/round mismatch by immediately exiting the loop; All the other writes will be rejected as well.
                Some(_) = pending_tasks.next() => {}
                msg = self.executed_tx_receiver.recv() => {
                    match msg {
                        Some((tx, tx_receipt, tx_state_diff)) => {
                            transactions_map.insert(tx.into(), (Some(tx_receipt), Some(tx_state_diff)));
                            pending_changes = true;
                        }
                        None => {
                            info!("Executed tx channel closed");
                            break;
                        }
                    }
                }
                msg = self.pre_confirmed_tx_receiver.recv() => {
                    match msg {
                        Some(txs) => {
                            // Skip transactions that were already executed, to avoid an unnecessary write.
                            for tx in txs {
                                let tx = tx.into();
                                if !transactions_map.contains_key(&tx) {
                                    transactions_map.insert(tx, (None, None));
                                    pending_changes = true;
                                }
                            }
                        }
                        None => {
                            info!("Pre confirmed tx channel closed");
                            break;
                        }
                    }
                }
            }
        }

        if pending_changes {
            let pre_confirmed_block =
                self.create_pre_confirmed_block(&transactions_map, write_iteration);
            self.cende_client.write_pre_confirmed_block(pre_confirmed_block).await?
        }

        // Wait for all pending tasks to complete gracefully.
        // TODO(noamsp): Add error handling and timeout.
        while pending_tasks.next().await.is_some() {}
        info!("Pre confirmed block writer finished");

        Ok(())
    }
}

#[cfg_attr(test, automock)]
pub trait PreConfirmedBlockWriterFactoryTrait: Send + Sync {
    fn create(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> (Box<dyn PreConfirmedBlockWriterTrait>, PreConfirmedTxSender, ExecutedTxSender);
}

pub struct PreConfirmedBlockWriterFactory {
    pub channel_capacity: usize,
    pub cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
}

impl PreConfirmedBlockWriterFactoryTrait for PreConfirmedBlockWriterFactory {
    fn create(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> (Box<dyn PreConfirmedBlockWriterTrait>, PreConfirmedTxSender, ExecutedTxSender) {
        // Initialize channels for communication between the pre confirmed block writer and the
        // block builder.
        let (executed_tx_sender, executed_tx_receiver) =
            tokio::sync::mpsc::channel(self.channel_capacity);
        let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::channel(self.channel_capacity);

        let cende_client = self.cende_client.clone();

        // TODO(noamsp): add the block metadata to the input.
        let pre_confirmed_block_writer_input = PreConfirmedBlockWriterInput {
            block_number,
            round: proposal_round,
            block_metadata: CendeBlockMetadata::empty_pending(),
        };

        let pre_confirmed_block_writer = Box::new(PreConfirmedBlockWriter::new(
            pre_confirmed_block_writer_input,
            pre_confirmed_tx_receiver,
            executed_tx_receiver,
            cende_client,
        ));
        (pre_confirmed_block_writer, pre_confirmed_tx_sender, executed_tx_sender)
    }
}

// TODO(noamsp): find a better name for this struct.
pub struct PreConfirmedBlockWriterInput {
    pub block_number: BlockNumber,
    pub round: Round,
    pub block_metadata: CendeBlockMetadata,
}
