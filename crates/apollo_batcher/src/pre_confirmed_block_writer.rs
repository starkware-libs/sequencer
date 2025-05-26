use std::sync::Arc;

use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

use crate::pre_confirmed_cende_client::PreConfirmedCendeClientTrait;

#[derive(Debug, Error)]
pub enum BlockWriterError {}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;
pub type PreConfirmedTxReceiver = tokio::sync::mpsc::UnboundedReceiver<Vec<TransactionHash>>;
pub type PreConfirmedTxSender = tokio::sync::mpsc::UnboundedSender<Vec<TransactionHash>>;
pub type ExecutedTxReceiver =
    tokio::sync::mpsc::UnboundedReceiver<Vec<(TransactionHash, TransactionReceipt)>>;
pub type ExecutedTxSender =
    tokio::sync::mpsc::UnboundedSender<Vec<(TransactionHash, TransactionReceipt)>>;

#[async_trait]
pub trait PreConfirmedBlockWriterTrait: Send {
    async fn run(&mut self) -> BlockWriterResult<()>;
}

pub struct PreConfirmedBlockWriter {
    _block_number: BlockNumber,
    _proposal_round: Round,
    _pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
    _executed_tx_receiver: ExecutedTxReceiver,
    _cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
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
            _block_number: block_number,
            _proposal_round: proposal_round,
            _pre_confirmed_tx_receiver: pre_confirmed_tx_receiver,
            _executed_tx_receiver: executed_tx_receiver,
            _cende_client: cende_client,
        }
    }
}

#[async_trait]
impl PreConfirmedBlockWriterTrait for PreConfirmedBlockWriter {
    async fn run(&mut self) -> BlockWriterResult<()> {
        todo!("Implement block writing logic")
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
