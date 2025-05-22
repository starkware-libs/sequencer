use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

use crate::pre_confirmed_cende_writer::{PreConfirmedCendeWriter, PreConfirmedCendeWriterFactory};

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
    async fn write_block(&mut self) -> BlockWriterResult<()>;
}

pub struct PreConfirmedBlockWriter {
    _pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
    _executed_tx_receiver: ExecutedTxReceiver,
    _cende_client: PreConfirmedCendeWriter,
}

impl PreConfirmedBlockWriter {
    pub fn new(
        pre_confirmed_tx_receiver: PreConfirmedTxReceiver,
        executed_tx_receiver: ExecutedTxReceiver,
        cende_client: PreConfirmedCendeWriter,
    ) -> Self {
        Self {
            _pre_confirmed_tx_receiver: pre_confirmed_tx_receiver,
            _executed_tx_receiver: executed_tx_receiver,
            _cende_client: cende_client,
        }
    }
}

#[async_trait]
impl PreConfirmedBlockWriterTrait for PreConfirmedBlockWriter {
    async fn write_block(&mut self) -> BlockWriterResult<()> {
        todo!("Implement block writing logic")
    }
}

pub trait PreConfirmedBlockWriterFactoryTrait: Send + Sync {
    fn create_pre_confirmed_block_writer(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> (Box<dyn PreConfirmedBlockWriterTrait>, PreConfirmedTxSender, ExecutedTxSender);
}

pub struct PreConfirmedBlockWriterFactory {
    pub cende_client_factory: PreConfirmedCendeWriterFactory,
}

impl PreConfirmedBlockWriterFactoryTrait for PreConfirmedBlockWriterFactory {
    fn create_pre_confirmed_block_writer(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> (Box<dyn PreConfirmedBlockWriterTrait>, PreConfirmedTxSender, ExecutedTxSender) {
        // Initialize channels for communication between the pre confirmed block writer and the
        // block builder.
        let (executed_tx_sender, executed_tx_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let cende_client = self
            .cende_client_factory
            .create_pre_confirmed_cende_writer(block_number, proposal_round);

        let pre_confirmed_block_writer = Box::new(PreConfirmedBlockWriter::new(
            pre_confirmed_tx_receiver,
            executed_tx_receiver,
            cende_client,
        ));
        (pre_confirmed_block_writer, pre_confirmed_tx_sender, executed_tx_sender)
    }
}
