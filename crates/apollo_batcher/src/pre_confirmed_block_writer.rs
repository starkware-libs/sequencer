use std::sync::Arc;

use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::info;

use crate::cende_client_types::{CendeBlockMetdata, StarknetClientTransactionReceipt};
use crate::pre_confirmed_cende_client::{
    CendeExecutedTxs,
    CendePreConfirmedTxs,
    CendeStartNewRound,
    PreConfirmedCendeClientError,
    PreConfirmedCendeClientTrait,
    PreConfirmedTransactionData,
};

#[derive(Debug, Error)]
pub enum BlockWriterError {
    #[error(transparent)]
    PreConfirmedCendeClientError(#[from] PreConfirmedCendeClientError),
}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;

pub type PreConfirmedTxReceiver = tokio::sync::mpsc::Receiver<Vec<TransactionHash>>;
pub type PreConfirmedTxSender = tokio::sync::mpsc::Sender<Vec<TransactionHash>>;

pub type ExecutedTxReceiver =
    tokio::sync::mpsc::Receiver<(TransactionHash, StarknetClientTransactionReceipt)>;
pub type ExecutedTxSender =
    tokio::sync::mpsc::Sender<(TransactionHash, StarknetClientTransactionReceipt)>;

/// Coordinates the flow of pre-confirmed transaction data during block proposal.
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
}

#[async_trait]
impl PreConfirmedBlockWriterTrait for PreConfirmedBlockWriter {
    async fn run(&mut self) -> BlockWriterResult<()> {
        self.cende_client
            .send_start_new_round(CendeStartNewRound {
                block_number: self.pre_confirmed_block_writer_input.block_number,
                round: self.pre_confirmed_block_writer_input.round,
            })
            .await?;
        loop {
            // TODO(noamsp): Manage the sending process independently so it doesn't block the loop.
            tokio::select! {
                msg = { self.executed_tx_receiver.recv() } => {
                    match msg {
                        Some((tx_hash, tx_receipt)) => self.cende_client
                            .write_executed_txs(CendeExecutedTxs {
                                block_number: self.pre_confirmed_block_writer_input.block_number,
                                round: self.pre_confirmed_block_writer_input.round,
                                executed_txs: [(tx_hash, PreConfirmedTransactionData {
                                    block_number: self.pre_confirmed_block_writer_input.block_number,
                                        round: self.pre_confirmed_block_writer_input.round,
                                        transaction_receipt: Some(tx_receipt),
                                    }),
                                ].into(),
                            })
                            .await?,
                        None => {
                            info!("Executed tx channel closed");
                            break;
                        }
                    }
                }
                msg = { self.pre_confirmed_tx_receiver.recv() } => {
                    match msg {
                        Some(txs) => self.cende_client
                            .write_pre_confirmed_txs(CendePreConfirmedTxs {
                                block_number: self.pre_confirmed_block_writer_input.block_number,
                                round: self.pre_confirmed_block_writer_input.round,
                                pre_confirmed_txs: txs.into_iter().map(|tx_hash| (tx_hash, PreConfirmedTransactionData {
                                    block_number: self.pre_confirmed_block_writer_input.block_number,
                                    round: self.pre_confirmed_block_writer_input.round,
                                    transaction_receipt: None,
                                })).collect(),
                            })
                            .await?,
                        None => {
                            info!("Pre confirmed tx channel closed");
                            break;
                        }
                    }
                }
            }
        }
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
            block_metadata: CendeBlockMetdata::empty_pending(),
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
    pub block_metadata: CendeBlockMetdata,
}
