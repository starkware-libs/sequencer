use std::sync::Arc;

use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::info;

use crate::pre_confirmed_cende_client::{
    PreConfirmedCendeClientError,
    PreConfirmedCendeClientTrait,
};

#[derive(Debug, Error)]
pub enum BlockWriterError {
    #[error(transparent)]
    PreConfirmedCendeClientError(#[from] PreConfirmedCendeClientError),
}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;

pub type PreConfirmedTxReceiver = tokio::sync::mpsc::UnboundedReceiver<Vec<TransactionHash>>;
pub type PreConfirmedTxSender = tokio::sync::mpsc::UnboundedSender<Vec<TransactionHash>>;

// TODO(noamsp): Change TransactionReceipt to TransactionExecutionInfo and translate into the
// receipt type that FGW uses.
pub type ExecutedTxReceiver =
    tokio::sync::mpsc::UnboundedReceiver<Vec<(TransactionHash, TransactionReceipt)>>;
pub type ExecutedTxSender =
    tokio::sync::mpsc::UnboundedSender<Vec<(TransactionHash, TransactionReceipt)>>;

/// Coordinates the flow of pre-confirmed transaction data during block proposal.
/// Listens for transaction updates from the block builder via dedicated channels and utilizes a
/// Cende client to communicate the updates to the Cende recorder.
#[async_trait]
#[cfg_attr(test, automock)]
pub trait PreConfirmedBlockWriterTrait: Send {
    async fn run(&mut self) -> BlockWriterResult<()>;
}

pub struct PreConfirmedBlockWriter {
    block_number: BlockNumber,
    proposal_round: Round,
    pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
    executed_tx_receiver: ExecutedTxReceiver,
    cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
}

impl PreConfirmedBlockWriter {
    pub fn new(
        block_number: BlockNumber,
        proposal_round: Round,
        pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
        executed_tx_receiver: ExecutedTxReceiver,
        cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
    ) -> Self {
        Self {
            block_number,
            proposal_round,
            pre_confirmed_tx_receiver,
            executed_tx_receiver,
            cende_client,
        }
    }
}

#[async_trait]
impl PreConfirmedBlockWriterTrait for PreConfirmedBlockWriter {
    async fn run(&mut self) -> BlockWriterResult<()> {
        self.cende_client.send_start_new_round(self.block_number, self.proposal_round).await?;
        loop {
            // TODO(noamsp): Manage the sending process independently so it doesn't block the loop.
            tokio::select! {
                msg = { self.executed_tx_receiver.recv() } => {
                    match msg {
                        Some(txs) => self.cende_client
                            .send_executed_txs(self.block_number, self.proposal_round, txs)
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
                            .send_pre_confirmed_txs(self.block_number, self.proposal_round, txs)
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
        // TODO(noamsp): Use bounded channels instead of unbounded channels.
        let (executed_tx_sender, executed_tx_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let cende_client = self.cende_client.clone();

        let pre_confirmed_block_writer = Box::new(PreConfirmedBlockWriter::new(
            block_number,
            proposal_round,
            pre_confirmed_tx_receiver,
            executed_tx_receiver,
            cende_client,
        ));
        (pre_confirmed_block_writer, pre_confirmed_tx_sender, executed_tx_sender)
    }
}
